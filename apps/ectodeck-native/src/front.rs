//! The Z10's page as the window shows it: what the page asks for, what the
//! keyboard answers, and the models the page is drawn from. Nothing here
//! knows about the window, so the offscreen check drives it the same way.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ectodeck_draw::hdr::Light;
use ectotype_core::watch::Watched;
use slint::{Color, Model, ModelRc, SharedString, VecModel};

use crate::device;
use crate::lighting::{self, Ctl};
use crate::overlays::Overlays;
use crate::z10::{self, Action, Page, View};
use crate::{AppWindow, Cell, CheckItem, EffectItem, KeyItem, LayerItem, LibGroup, LibRow, MenuRow, ProfileItem};

/// What the page asked for, done once its event has been handled.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// the rebindings shown on the keyboard, or not
    ShowRebindings,
    Move,
    Minimise,
    Maximise,
    Close,
    View(i32),
    Profile(i32),
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
    Go(String),
    Rail(String),
    /// the Macro Editor, on the macro of this layer of the chosen key, or -1
    MacroEditor(i32),
    /// time to read the sliders again, and the display's text
    PollSliders,
    PollLcd,
    PollActive,
    Effect(i32),
    CellPick(i32, i32, f32, f32, f32),
    CellSlide(i32, f32),
    /// time to send the preview again: the effect every two seconds, streamed colours 14 times a second
    PreviewTick,
    StreamTick,
    /// time to save what has changed, once it has stopped changing
    AutoSave,
    /// undo the last change, or do again the last one undone
    Undo(bool),
    PkSv(f32, f32),
    PkHue(f32),
    PkField(i32, String),
    PkWell(slint::Color),
    PkEnter,
    PkDismiss,
    CellTyped(i32, String),
    /// the profile being edited, given a new name
    Rename(String),
    /// the layer the Keys tab edits: 0 Base, 1 Function
    Layer(i32),
    /// the Key library: a group opened or closed, a search
    LibToggle(String),
    LibFind(String),
    /// one of Game mode's keys blocked or not
    Game(i32),
    /// the chosen key's kind of job, from its switch, under the switch's part
    Kind(i32, f32, f32, f32),
    /// a slider chosen in the library (0 none), and what it controls
    SliderSel(i32),
    SliderBind(i32),
    /// the pointer over an area's chip on the Lighting page, or off it: its place in ZONES
    AreaHover(i32, bool),
    /// time to let go of a zone the pointer left, unless it went on to another
    HoverSettle,
    /// the pointer onto (true) or off a key on the keyboard's picture
    KeyHover(i32, bool),
    /// the Screen tab: Pause or Play; game launches breaking in, or not; time
    /// for the playlist's picture of a launch to move on
    LcdPause,
    LcdAnnounce,
    LcdTick,
}

/// The colour picker, open on a swatch: what it sets, the colour it opened
/// on, and the colour now, as hue (0 to 360), saturation and brightness (0 to 1).
struct Pick {
    ctl: Ctl,
    index: usize,
    start: [u8; 3],
    hsv: [f32; 3],
    rgb: [u8; 3],
    anchor: (f32, f32, f32),
}

const PRESETS: [[u8; 3]; 9] = [[255, 255, 255], [255, 0, 0], [255, 96, 0], [255, 200, 0], [0, 255, 0], [0, 255, 200], [0, 120, 255], [120, 0, 255], [255, 0, 180]];

pub(crate) fn rgb2hsv([r, g, b]: [u8; 3]) -> [f32; 3] {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let d = mx - mn;
    let mut h = 0.0;
    if d > 0.0 {
        h = if mx == r { ((g - b) / d) % 6.0 } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
    }
    [(h * 60.0 + 360.0) % 360.0, if mx > 0.0 { d / mx } else { 0.0 }, mx]
}

pub(crate) fn hsv2rgb([h, s, v]: [f32; 3]) -> [u8; 3] {
    let f = |n: f32| {
        let k = (n + h / 60.0) % 6.0;
        (255.0 * (v - v * s * k.min(4.0 - k).clamp(0.0, 1.0))).round() as u8
    };
    [f(5.0), f(3.0), f(1.0)]
}

/// A rounded box's picture: `f` gives each pixel's colour from its place (0 to 1 across and down).
fn rounded(w: u32, h: u32, r: f32, f: impl Fn(f32, f32) -> [f32; 3]) -> slint::Image {
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
    let (fw, fh) = (w as f32, h as f32);
    for (i, p) in buf.make_mut_slice().iter_mut().enumerate() {
        let (px, py) = ((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5);
        let qx = (r - px).max(px - (fw - r)).max(0.0);
        let qy = (r - py).max(py - (fh - r)).max(0.0);
        let a = if qx > 0.0 && qy > 0.0 { (r - (qx * qx + qy * qy).sqrt() + 0.5).clamp(0.0, 1.0) } else { 1.0 };
        let c = f(px / fw, py / fh);
        *p = slint::Rgba8Pixel { r: (c[0] * a).round() as u8, g: (c[1] * a).round() as u8, b: (c[2] * a).round() as u8, a: (a * 255.0).round() as u8 };
    }
    slint::Image::from_rgba8_premultiplied(buf)
}

/// The picker's square for a hue: white to the hue across, then to black downward, in sRGB as CSS mixes them.
pub(crate) fn sv_square(hue: f32, scale: f32) -> slint::Image {
    let pure = hsv2rgb([hue, 1.0, 1.0]).map(|v| v as f32);
    rounded((252.0 * scale) as u32, (160.0 * scale) as u32, 8.0 * scale, |x, y| [0, 1, 2].map(|k| (255.0 + (pure[k] - 255.0) * x) * (1.0 - y)))
}

pub(crate) fn hue_bar(scale: f32) -> slint::Image {
    let stops: [(f32, [f32; 3]); 7] =
        [(0.0, [255.0, 0.0, 0.0]), (0.17, [255.0, 255.0, 0.0]), (0.33, [0.0, 255.0, 0.0]), (0.5, [0.0, 255.0, 255.0]), (0.67, [0.0, 0.0, 255.0]), (0.83, [255.0, 0.0, 255.0]), (1.0, [255.0, 0.0, 0.0])];
    rounded((252.0 * scale) as u32, (12.0 * scale) as u32, 6.0 * scale, |x, _| {
        let i = stops.iter().rposition(|(t, _)| *t <= x).unwrap_or(0).min(5);
        let ((t0, a), (t1, b)) = (stops[i], stops[i + 1]);
        let f = ((x - t0) / (t1 - t0)).clamp(0.0, 1.0);
        [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * f)
    })
}

pub(crate) fn color([r, g, b]: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(r, g, b)
}

/// What the window itself must do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowAsk {
    Move,
    Minimise,
    Maximise,
    Close,
    /// open the Macro Editor's window, on this macro (0: as it was)
    OpenMacros(u32),
    /// open the Key lab's window
    OpenLab,
}

/// The Plays menu: the six choices, a rule between those that start with the
/// profile and those that start on a key press, and a pair stored that is
/// none of them, after its own rule, ticked, so it is never hidden.
fn plays_rows(page: &Page) -> Vec<MenuRow> {
    let l = &page.draft()["lighting"];
    let e = &l[l["mode"].as_str().unwrap_or("off")];
    let (start, stop) = (e["start"].as_u64().unwrap_or(0), e["stop"].as_u64().unwrap_or(0));
    let at = lighting::plays_of(start, stop);
    let rule = || MenuRow { kind: 2, label: "".into(), tag: "".into(), tick: false, sub: "".into() };
    let mut rows = Vec::new();
    for (j, (label, _, _)) in lighting::PLAYS.iter().enumerate() {
        if j == 3 {
            rows.push(rule());
        }
        rows.push(MenuRow { kind: 0, label: (*label).into(), tag: "".into(), tick: at == Some(j), sub: "".into() });
    }
    if at.is_none() {
        rows.push(rule());
        rows.push(MenuRow { kind: 0, label: lighting::plays_label(start, stop).into(), tag: "".into(), tick: true, sub: "".into() });
    }
    rows
}

const LAYERS: [(&str, &str); 2] = [("keys", "When pressed"), ("fnkeys", "When Fn is held")];

/// The action menu's pages under "Other": each one's title and its list.
const PAGES: [&str; 5] = ["A key", "A media key", "A mouse button", "Switch profile", "Keyboard function"];
/// the action menu's page of macros, opened from the key's Macro switch
const MACRO_PAGE: usize = 5;

/// The Key library's groups after the sliders and the buttons, and the keys in each by their ids.
const LIB_GROUPS: [&str; 7] = ["Macro keys", "F-keys", "Number row", "Letters and symbols", "Modifiers and space", "Navigation", "Number pad"];
fn lib_group_of(id: &str) -> &'static str {
    match id {
        "GameMode" | "MultimediaStop" | "MultimediaPreviousTrack" | "MultimediaPlayPause" | "MultimediaNextTrack" => "Buttons",
        "E1" | "E2" | "E3" | "E4" | "E5" => "Macro keys",
        "ESC" | "PrintScreen" | "ScrollLock" | "PauseBreak" => "F-keys",
        f if f.len() <= 3 && f.starts_with('F') && f[1..].parse::<u8>().is_ok() => "F-keys",
        "GraveAccent" | "Minus" | "Equals" | "Backspace" => "Number row",
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => "Number row",
        "Tab" | "CapsLock" | "LSHIFT" | "RSHIFT" | "LCONTROL" | "RCONTROL" | "WIN" | "LALT" | "RALT" | "FN" | "Menu" | "Space" | "Enter" => "Modifiers and space",
        "Insert" | "Home" | "PageUp" | "Delete" | "End" | "PageDown" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "E6" | "E7" => "Navigation",
        k if k.starts_with("Keypad") || k == "NumLock" => "Number pad",
        _ => "Letters and symbols",
    }
}

/// The Key library's groups and their keys, in the order the keyboard has them.
fn lib_groups(p: &Page) -> Vec<(String, Vec<z10::Key>)> {
    let mut out: Vec<(String, Vec<z10::Key>)> = ["Buttons"].iter().chain(LIB_GROUPS.iter()).map(|g| (g.to_string(), Vec::new())).collect();
    for k in &p.layout {
        let g = lib_group_of(&k.id);
        if let Some(e) = out.iter_mut().find(|(n, _)| n == g) {
            e.1.push(k.clone());
        }
    }
    out
}

/// A key's cap, short, for the Key library.
fn lib_cap(id: &str, name: &str) -> String {
    let fixed = match id {
        "GameMode" => "G/PC",
        "MultimediaStop" => "Stop",
        "MultimediaPreviousTrack" => "Prev",
        "MultimediaPlayPause" => "Play",
        "MultimediaNextTrack" => "Next",
        "WIN" => "Win",
        "LCONTROL" | "RCONTROL" => "Ctrl",
        "LALT" | "RALT" => "Alt",
        "LSHIFT" | "RSHIFT" => "Shift",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "NumLock" => "Num",
        "KeypadEnter" => "Ent",
        "KeypadPlus" => "+",
        "KeypadMinus" => "−",
        "KeypadMultiply" => "*",
        "KeypadDivide" => "/",
        "KeypadPeriod" => ".",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.into();
    }
    if let Some(d) = id.strip_prefix("Keypad") {
        return d.into();
    }
    if name.chars().count() <= 3 {
        return name.into();
    }
    name.split_whitespace().next().unwrap_or(name).chars().take(4).collect()
}

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

/// The file picker open for a typeface, in place of a part of a legend's picture.
const TYPEFACE_PICK: usize = usize::MAX;

pub struct Front {
    pub ui: AppWindow,
    pub page: Page,
    kb: Option<mpsc::Sender<device::Ask>>,
    pub asks: Rc<RefCell<Vec<Ask>>>,
    menu: Option<Menu>,
    /// the layer the Keys tab edits: 0 Base (when pressed), 1 Function (Fn held)
    pub layer: usize,
    /// the Key library: the groups closed, what is searched for, the slider chosen (1 dimmer, 2 volume)
    lib_shut: std::collections::HashSet<String>,
    lib_query: String,
    /// saving as things change: each profile's draft as last seen, when it last
    /// changed, the profile being saved and its draft as sent, what can be
    /// undone and done again (a profile and its draft before the change), and
    /// when the message showing now was put up
    last_drafts: HashMap<u8, serde_json::Value>,
    changed_at: HashMap<u8, Instant>,
    saving: Option<(u8, serde_json::Value)>,
    undo: Vec<(u8, serde_json::Value)>,
    redo: Vec<(u8, serde_json::Value)>,
    undo_mark: Option<(u8, Instant)>,
    status_at: Option<Instant>,
    /// the key or slider (-1 the dimmer, -2 the volume slider) whose group was
    /// last opened to show it: a group opens once when something in it is
    /// chosen, and after that its heading closes it again
    lib_revealed: Option<i32>,
    lib_rows: HashMap<String, Rc<VecModel<LibRow>>>,
    /// the screen's light and built-in screens the keyboard was last sent (or
    /// had when it was read), so a change to them shows on it at once
    screen_sent: Option<([u8; 4], u8)>,
    slider_sel: usize,
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
    /// a reading of the keyboard's profile asked for, not yet heard
    active_asked: bool,
    /// the lowest and highest reading seen for each slider, which widen as the
    /// sliders are moved, so a knob reaches both ends when its slider does
    slider_ends: [[f64; 2]; 2],
    _timers: Vec<slint::Timer>,
    /// the Lighting page: the colour Per key paints with, whether the keyboard
    /// is being sent the preview, and the pieces of the tile as last laid out
    paint: [u8; 3],
    previewing: bool,
    preview_asked: bool,
    icons: Option<(f32, Vec<slint::Image>)>,
    cells: Rc<VecModel<Cell>>,
    /// the Where and when box's pieces; the areas chosen for each profile's
    /// effects, kept while the whole keyboard is lit; the dropdown open; the
    /// area the pointer is over, which the keyboard shows
    place_cells: Rc<VecModel<Cell>>,
    areas: HashMap<(u8, String), u64>,
    light_menu: Option<Ctl>,
    pub hover_area: Option<usize>,
    /// the key the pointer is over on the keyboard's picture
    picture_key: Option<u32>,
    /// a zone's chip, or a key on the picture, the pointer has just left, and
    /// when: it stays lit a moment, so crossing the gap to the next one does
    /// not blink the keyboard
    chip_left: Option<(usize, Instant)>,
    key_left: Option<(u32, Instant)>,
    pub scale: f32,
    pick: Option<Pick>,
    overlay_model: Rc<VecModel<crate::Overlay>>,
    /// what each slider controls, which slider's menu is open, and what the
    /// sliders last read and last set
    bind: [String; 2],
    bind_menu: Option<(usize, (f32, f32, f32))>,
    /// a slider set to Nothing because the other took its job: the job it had
    idled: [Option<String>; 2],
    last_sliders: Option<serde_json::Value>,
    held: Option<u8>,
    sent: [Option<f64>; 2],
    /// quitting: when the keyboard was asked to take its sliders back, which
    /// Ectodeck waits a moment for before it goes
    released: Option<Instant>,
    /// Ectodeck's own settings, the weather every device shares, and the
    /// place search: what is typed, when, and the places found
    pub settings: crate::settings::Settings,
    weather: ectodeck_weather::Feed,
    place_text: String,
    typed_at: Option<Instant>,
    found: Vec<ectodeck_weather::Place>,
    found_tx: mpsc::Sender<(String, Vec<ectodeck_weather::Place>)>,
    found_rx: mpsc::Receiver<(String, Vec<ectodeck_weather::Place>)>,
    place_menu: bool,
    /// the screen's levels, for the Peak nits slider
    pub levels: (f64, f64),
    /// Ectodeck is to start again, on this place, for a change to what it may use
    pub restart: Option<String>,
    /// the Macro Editor's window, once it has been opened
    pub macros: Option<crate::macro_page::MacroPage>,
    /// the Key lab's window, once it has been opened
    pub lab: Option<crate::lab::LabPage>,
    /// the keys whose caps have tops pale enough that a new meaning printed
    /// on them reads in dark ink, by their place in the keyboard's reports
    pale: std::collections::HashSet<u32>,
    /// the desktop's file picker, open for a picture for a part of a key's legend
    picking: Option<(usize, std::sync::mpsc::Receiver<Option<std::path::PathBuf>>)>,
    /// whether a rebound key's new action is printed on it on the keyboard
    rebindings: bool,
    /// when the sliders were last read, and the time between readings, smoothed
    last_reading: Option<Instant>,
    reading_gap: f32,
    /// the keyboard as the kernel last showed it, and when
    dev_status: Option<(Instant, serde_json::Value)>,
    /// the screen: Ectodeck's player, what it last said, whether it is
    /// paused, the launch the playlist plays as its picture (its frames, the
    /// one showing and since when), and what the pictures were last drawn from
    pub player: Option<crate::lcd_player::Player>,
    lcd: crate::lcd_player::News,
    lcd_paused: bool,
    launch_frames: Vec<(Vec<String>, u32)>,
    launch_at: (usize, Instant),
    lcd_drawn: Option<(Vec<String>, [u8; 3], u8, u32)>,
    launch_drawn: Option<(usize, [u8; 3], u8, u32)>,
}

/// A profile's colour sample: how many colours, and the colours.
fn swatch(cols: &[[f64; 3]]) -> (i32, [Color; 4]) {
    let c = |i: usize| cols.get(i).or(cols.last()).map_or(Color::from_rgb_u8(0, 0, 0), |v| Color::from_rgb_u8(v[0] as u8, v[1] as u8, v[2] as u8));
    (cols.len().clamp(1, 4) as i32, [c(0), c(1), c(2), c(3)])
}

impl Front {
    pub fn new(ui: AppWindow, kb: Option<mpsc::Sender<device::Ask>>, controlled: bool, settings: crate::settings::Settings) -> Front {
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
        on!(on_undo, |redo| Ask::Undo(redo));
        on!(on_key_clicked, |p| Ask::Key(p));
        on!(on_board_clicked, Ask::Board);
        on!(on_show_rebindings, Ask::ShowRebindings);
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
        on!(on_macro_editor, |l| Ask::MacroEditor(l));
        on!(on_effect, |i| Ask::Effect(i));
        on!(on_cell_pick, |id, i, x, y, h| Ask::CellPick(id, i, x, y, h));
        on!(on_cell_slide, |id, v| Ask::CellSlide(id, v));
        on!(on_pk_sv, |x, y| Ask::PkSv(x, y));
        on!(on_pk_hue_at, |x| Ask::PkHue(x));
        on!(on_pk_field, |i, t| Ask::PkField(i, t.to_string()));
        on!(on_pk_well, |c| Ask::PkWell(c));
        on!(on_pk_enter, Ask::PkEnter);
        on!(on_cell_typed, |id, t| Ask::CellTyped(id, t.to_string()));
        on!(on_pk_dismiss, Ask::PkDismiss);
        on!(on_rename_profile, |t| Ask::Rename(t.to_string()));
        on!(on_layer_changed, |i| Ask::Layer(i));
        on!(on_lib_toggle, |g| Ask::LibToggle(g.to_string()));
        on!(on_lib_find, |t| Ask::LibFind(t.to_string()));
        on!(on_game_toggled, |i| Ask::Game(i));
        on!(on_kind_picked, |i, x, y, h| Ask::Kind(i, x, y, h));
        on!(on_slider_chosen_at, |i| Ask::SliderSel(i));
        on!(on_slider_bind_at, |i| Ask::SliderBind(i));
        on!(on_area_hover, |i, on| Ask::AreaHover(i, on));
        on!(on_key_hovered, |p, on| Ask::KeyHover(p, on));
        on!(on_lcd_pause, Ask::LcdPause);
        on!(on_lcd_announce_toggled, Ask::LcdAnnounce);
        let (ftx, frx) = mpsc::channel();
        let page = Page::new();
        ui.set_units(page.view_box.2 as f32);
        ui.set_units_high(page.view_box.3 as f32);
        // the sliders ten times a second, the display's text every two seconds, as the web app read them
        let timers = [
            (Duration::from_millis(100), Ask::PollSliders),
            (Duration::from_secs(2), Ask::PollLcd),
            (Duration::from_millis(500), Ask::PollActive),
            (Duration::from_secs(2), Ask::PreviewTick),
            (Duration::from_millis(70), Ask::StreamTick),
            (Duration::from_millis(250), Ask::AutoSave),
            (Duration::from_millis(50), Ask::HoverSettle),
            (Duration::from_millis(100), Ask::LcdTick),
        ]
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
            layer: 0,
            lib_shut: LIB_GROUPS.iter().map(|g| g.to_string()).chain(["Sliders".to_string(), "Buttons".to_string()]).collect(),
            lib_query: String::new(),
            last_drafts: HashMap::new(),
            changed_at: HashMap::new(),
            saving: None,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_mark: None,
            status_at: None,
            lib_revealed: None,
            lib_rows: HashMap::new(),
            screen_sent: None,
            slider_sel: 0,
            listening: None,
            shut: None,
            t0: Instant::now(),
            last_lights: None,
            pale: Default::default(),
            picking: None,
            rebindings: crate::uistate::get("show_rebindings").and_then(|v| v.as_bool()).unwrap_or(true),
            failed_at: None,
            controlled,
            overlays: Overlays::new(),
            sliders_asked: false,
            active_asked: false,
            slider_ends,
            _timers: timers,
            paint: lighting::PRESETS[0],
            previewing: false,
            preview_asked: false,
            icons: None,
            cells: Rc::new(VecModel::default()),
            place_cells: Rc::new(VecModel::default()),
            areas: HashMap::new(),
            light_menu: None,
            hover_area: None,
            picture_key: None,
            chip_left: None,
            key_left: None,
            scale: 1.0,
            pick: None,
            overlay_model: Rc::new(VecModel::default()),
            bind: crate::uistate::get("slider_bind").and_then(|v| serde_json::from_value(v).ok()).unwrap_or(["own".into(), "own".into()]),
            bind_menu: None,
            idled: crate::uistate::get("slider_idled").and_then(|v| serde_json::from_value(v).ok()).unwrap_or([None, None]),
            last_sliders: None,
            held: None,
            sent: [None, None],
            released: None,
            weather: ectodeck_weather::Feed::start(),
            place_text: settings.get("weather.place").and_then(|p| p["name"].as_str().map(String::from)).unwrap_or_default(),
            settings,
            typed_at: None,
            found: Vec::new(),
            found_tx: ftx,
            found_rx: frx,
            place_menu: false,
            levels: (254.0, 1000.0),
            restart: None,
            macros: None,
            lab: None,
            last_reading: None,
            reading_gap: 120.0,
            dev_status: None,
            player: None,
            lcd: crate::lcd_player::News { rows: Vec::new(), sequence: None, state: crate::lcd_player::State::Missing },
            lcd_paused: false,
            launch_frames: launch_frames(),
            launch_at: (0, Instant::now()),
            lcd_drawn: None,
            launch_drawn: None,
        };
        f.weather.set_place(weather_place(&f.settings));
        f.ui.set_overlays(ModelRc::from(f.overlay_model.clone()));
        f.ui.set_pk_presets(ModelRc::new(VecModel::from(PRESETS.iter().map(|c| color(*c)).collect::<Vec<_>>())));
        f.ui.set_cells(ModelRc::from(f.cells.clone()));
        f.ui.set_place_cells(ModelRc::from(f.place_cells.clone()));
        f.ui.set_lcd_announce(f.settings.on("devices.z10.screen.announce"));
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

    fn status(&mut self, text: impl Into<SharedString>, kind: i32) {
        self.ui.set_status_text(text.into());
        self.ui.set_status_kind(kind);
        self.ui.set_undo_shown(false);
        self.status_at = Some(Instant::now());
    }

    /// The keyboard answered.
    pub fn heard(&mut self, h: device::Heard) {
        match h {
            device::Heard::State(Ok(dev)) => {
                self.failed_at = None;
                self.page.loaded(dev);
                // the keyboard already shows the screen of the profile it is using
                let n = self.page.active();
                self.screen_sent = self.page.dev.as_ref().map(|d| screen_of(&d["profiles"][n.to_string()]["lighting"]));
                self.tell_modes();
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
                let sent = self.saving.take().filter(|(m, _)| *m == n).map(|(_, d)| d);
                match r {
                    Ok(p) => {
                        // changed again while it was saving: the newer draft stays, and saves next
                        let unchanged = sent.as_ref() == self.page.drafts.get(&n);
                        let was_custom = sent.as_ref().is_some_and(|d| d["lighting"]["mode"] == "custom");
                        self.page.saved_profile(n, p, unchanged);
                        if unchanged && was_custom {
                            if let Some(d) = self.page.drafts.get_mut(&n) {
                                d["lighting"]["mode"] = serde_json::json!("custom");
                            }
                        }
                        if let Some(d) = self.page.drafts.get(&n) {
                            self.last_drafts.insert(n, d.clone());
                        }
                        self.status("Saved", 1);
                        self.ui.set_undo_shown(!self.undo.is_empty());
                    }
                    Err(e) => {
                        // not tried again until it changes again
                        self.changed_at.remove(&n);
                        self.status(format!("{} was not saved to the keyboard: {e}", self.page.profile_name(n)), 2);
                    }
                }
            }
            device::Heard::Done => return,
            h @ (device::Heard::Macros(..) | device::Heard::KeysSaved(..)) => {
                if let Some(m) = &mut self.macros {
                    m.heard(&mut self.page, h);
                }
            }
            device::Heard::Preview(r) => {
                self.preview_asked = false;
                match r {
                    Ok(v) => {
                        self.ui.set_live(self.previewing && !v["mode"].is_null() && v["mode"] != false);
                        if let Some(e) = v["error"].as_str() {
                            self.status(format!("The keyboard stopped showing the preview: {e}"), 2);
                        }
                    }
                    Err(e) => {
                        self.ui.set_live(false);
                        self.previewing = false;
                        self.status(format!("Preview unavailable: {e}"), 2);
                    }
                }
                return;
            }
            device::Heard::Sliders(r) => {
                self.sliders_asked = false;
                // how often readings really come, which the knobs' glide lasts
                if let Some(t) = self.last_reading.replace(Instant::now()) {
                    let gap = t.elapsed().as_secs_f32() * 1000.0;
                    if gap < 400.0 {
                        self.reading_gap = self.reading_gap * 0.7 + gap * 0.3;
                        self.ui.set_knob_glide(self.reading_gap.clamp(60.0, 250.0) as i64);
                    }
                }
                if let Ok(v) = r {
                    self.move_knobs(&v);
                }
                return;
            }
            // the keyboard switched profiles itself: the page follows it, keeping every profile's unsaved changes
            device::Heard::ActiveNow(r) => {
                self.active_asked = false;
                if let Ok(Some(n)) = r {
                    if (1..=5).contains(&n) && n != self.page.active() {
                        self.page.set_active(n);
                        self.page.choose(n);
                        self.status(format!("The keyboard switched to profile {n}."), 0);
                    }
                }
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
        // (and, while the desktop's file picker is open, often enough to take its answer)
        let picking = self.picking.as_ref().map(|_| Duration::from_millis(100));
        [self.failed_at.map(|t| Duration::from_secs(3).saturating_sub(t.elapsed()) + Duration::from_millis(10)), picking].into_iter().flatten().min()
    }

    /// The place search: once typing has paused, the places matching it,
    /// found away from the window's thread, in this computer's language.
    fn search_places(&mut self) {
        if let Some(t) = self.typed_at {
            if t.elapsed() >= Duration::from_millis(300) {
                self.typed_at = None;
                let q = self.place_text.trim().to_string();
                if q.is_empty() {
                    self.close_place_menu();
                    return;
                }
                let tx = self.found_tx.clone();
                let lang = ectodeck_weather::units::language(&crate::locale());
                std::thread::spawn(move || {
                    let found = ectodeck_weather::search(&q, &lang).unwrap_or_default();
                    let _ = tx.send((q, found));
                });
            }
        }
        while let Ok((q, found)) = self.found_rx.try_recv() {
            if q != self.place_text.trim() {
                continue; // typed on since
            }
            self.found = found;
            self.open_place_menu();
        }
    }

    fn open_place_menu(&mut self) {
        let rows: Vec<MenuRow> = if self.found.is_empty() {
            vec![MenuRow { kind: 1, label: "No place by that name".into(), tag: "".into(), tick: false, sub: "".into() }]
        } else {
            self.found
                .iter()
                .map(|p| MenuRow {
                    kind: 0,
                    label: if !p.latin.is_empty() && p.latin != p.name { format!("{} ({})", p.name, p.latin) } else { p.name.clone() }.into(),
                    tag: "".into(),
                    tick: false,
                    sub: [p.region.as_str(), p.country.as_str()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(", ").into(),
                })
                .collect()
        };
        let list: f32 = rows.iter().map(|r| if r.kind == 1 { 28.0 } else { 33.0 }).sum();
        // under the place field, as wide as it
        let field = (0..self.cells.row_count()).filter_map(|i| self.cells.row_data(i)).find(|c| c.kind == lighting::kind::FIELD && lighting::ctl(c.id) == Ctl::Place);
        let Some(f) = field else { return };
        let scroll = self.ui.get_settings_scroll();
        let (x, y) = (self.ui.get_rail_width() + f.x, 36.0 + f.y + scroll);
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(6.0 + list.min(320.0) + 6.0);
        self.ui.set_menu_w(f.w);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        self.ui.set_menu_ax(x);
        self.ui.set_menu_ay(y);
        self.ui.set_menu_ah(f.h);
        self.ui.set_menu_top(y + f.h + 6.0);
        self.ui.set_menu_max(320.0 + 12.0);
        self.ui.set_menu_open(true);
        self.place_menu = true;
    }

    fn close_place_menu(&mut self) {
        if self.place_menu {
            self.place_menu = false;
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_menu_w(300.0);
        }
    }

    /// Do what the page asked; what only the window can do is handed back.
    pub fn act(&mut self) -> Vec<WindowAsk> {
        self.search_places();
        self.settings.flush(false);
        let asks: Vec<Ask> = self.asks.borrow_mut().drain(..).collect();
        let mut window = Vec::new();
        // whether any ask changed the page (a timer's changing nothing must
        // not hide a change an earlier ask made)
        let mut any = false;
        for ask in asks {
            let mut changed = true;
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
                    self.page.view = [View::Keys, View::Lighting, View::Display, View::Screen][i.clamp(0, 3) as usize];
                    if self.page.view == View::Lighting {
                        self.start_preview();
                    } else {
                        self.stop_preview();
                    }
                }
                Ask::Effect(i) => {
                    if let (Some((mode, _)), Some(d)) = (z10::EFFECTS.get(i as usize), self.page.drafts.get_mut(&self.page.editing)) {
                        d["lighting"]["mode"] = serde_json::json!(mode);
                    }
                    self.push_preview();
                }
                Ask::CellPick(id, i, x, y, h) => self.cell_pick(lighting::ctl(id), i.max(0) as usize, (x, y, h)),
                Ask::CellSlide(id, v) => {
                    match lighting::ctl(id) {
                        Ctl::PeakNits => {
                            let nits = crate::ecto_settings::peak_at(v, self.levels.0, self.levels.1);
                            self.settings.set_soon("hdr.glow_limit", nits.map_or(serde_json::Value::Null, |n| serde_json::json!(n)));
                        }
                        Ctl::LcdBrightness => crate::display::act(&mut self.page, Ctl::LcdBrightness, 0, v, None),
                        c => lighting::act(&mut self.page, c, 0, v),
                    }
                    self.push_preview();
                }
                Ask::LcdPause => {
                    self.lcd_paused = !self.lcd_paused;
                    if let Some(pl) = &self.player {
                        pl.tell(crate::lcd_player::Control::Paused(self.lcd_paused));
                    }
                }
                Ask::LcdAnnounce => {
                    let on = !self.settings.on("devices.z10.screen.announce");
                    if let Err(e) = self.settings.set("devices.z10.screen.announce", serde_json::json!(on)) {
                        self.status(format!("The setting was not saved: {e}"), 2);
                    }
                    self.ui.set_lcd_announce(on);
                    if let Some(pl) = &self.player {
                        pl.tell(crate::lcd_player::Control::Announce(on));
                    }
                }
                Ask::LcdTick => {
                    // the playlist's picture of a launch, frame by frame, round and round
                    if self.page.view == View::Screen && self.ui.get_place() == "z10" && !self.launch_frames.is_empty() {
                        let (i, since) = self.launch_at;
                        // a pause at the end before it plays again, as the mock had
                        let ms = self.launch_frames[i].1 as u128 + if i + 1 == self.launch_frames.len() { 2500 } else { 0 };
                        if since.elapsed().as_millis() >= ms {
                            self.launch_at = ((i + 1) % self.launch_frames.len(), Instant::now());
                            self.draw_screen();
                        }
                    }
                    continue;
                }
                Ask::KeyHover(pos, on) => {
                    let pos = pos.max(0) as u32;
                    let was = self.picture_area();
                    if on {
                        self.picture_key = Some(pos);
                        self.key_left = None;
                    } else if self.picture_key == Some(pos) {
                        // let go of in a moment, unless the pointer reaches another key first
                        self.key_left = Some((pos, Instant::now()));
                        continue;
                    }
                    // only a change of area lit on the picture changes the page
                    if self.picture_area() == was {
                        continue;
                    }
                }
                Ask::AreaHover(i, on) => {
                    let i = i.max(0) as usize;
                    if on {
                        self.hover_area = Some(i);
                        self.chip_left = None;
                    } else if self.hover_area == Some(i) {
                        // let go of in a moment, unless the pointer reaches another chip first
                        self.chip_left = Some((i, Instant::now()));
                        continue;
                    }
                }
                Ask::HoverSettle => {
                    const HOLD: Duration = Duration::from_millis(150);
                    let was = self.area_keys();
                    if self.chip_left.is_some_and(|(i, t)| t.elapsed() >= HOLD && self.hover_area == Some(i)) {
                        self.chip_left = None;
                        self.hover_area = None;
                    }
                    if self.key_left.is_some_and(|(k, t)| t.elapsed() >= HOLD && self.picture_key == Some(k)) {
                        self.key_left = None;
                        self.picture_key = None;
                    }
                    changed = self.area_keys() != was;
                }
                Ask::Rename(t) => crate::display::act(&mut self.page, Ctl::Name, 0, 0.0, Some(&t)),
                Ask::Layer(i) => {
                    self.layer = i.clamp(0, 1) as usize;
                    self.listening = None;
                    self.close_menu();
                    // the sliders are no part of the Function layer
                    if self.layer == 1 {
                        self.slider_sel = 0;
                    }
                }
                Ask::LibToggle(g) => {
                    if !self.lib_shut.remove(&g) {
                        self.lib_shut.insert(g);
                    }
                }
                Ask::LibFind(t) => self.lib_query = t,
                Ask::Game(i) => {
                    let blocked = !self.page.draft()["lighting"]["game_keys"].as_u64().unwrap_or(0) & 0x7f;
                    let on = crate::display::GAME_KEYS.get(i as usize).is_some_and(|g| blocked & g.0 != 0);
                    crate::display::act(&mut self.page, Ctl::GameKey, i as usize, if on { 0.0 } else { 1.0 }, None);
                }
                Ask::Kind(i, x, y, h) => {
                    // the parts of the switch, as the menu's pages are; Nothing at once
                    match i {
                        6 => {
                            self.close_menu();
                            self.assign(self.layer, [0, 0, 0, 0]);
                        }
                        _ => {
                            let page = [0, 1, 2, MACRO_PAGE, 3, 4][i.clamp(0, 5) as usize];
                            self.open_menu_on(self.layer, (x, y, h), Some(page));
                        }
                    }
                }
                Ask::SliderSel(i) => {
                    self.slider_sel = i.clamp(0, 2) as usize;
                    if self.slider_sel != 0 {
                        self.page.sel = None;
                        self.listening = None;
                        self.close_menu();
                    }
                }
                Ask::SliderBind(i) => {
                    if self.slider_sel != 0 {
                        let s = self.slider_sel - 1;
                        if let Some((v, _)) = crate::display::BIND_CHOICES[s].get(i as usize) {
                            self.set_slider_bind(s, v);
                        }
                    }
                }
                Ask::CellTyped(id, t) => match lighting::ctl(id) {
                    Ctl::Name => crate::display::act(&mut self.page, Ctl::Name, 0, 0.0, Some(&t)),
                    Ctl::Place => {
                        // searched once typing pauses, as the web page did
                        self.place_text = t;
                        self.typed_at = Some(Instant::now());
                        changed = false;
                    }
                    _ => {}
                },
                Ask::PkSv(x, y) => {
                    if let Some(p) = &self.pick {
                        let h = p.hsv[0];
                        self.pick_set([h, x, 1.0 - y], None, None);
                    }
                }
                Ask::PkHue(x) => {
                    if let Some(p) = &self.pick {
                        let [_, sv, v] = p.hsv;
                        self.pick_set([(x * 360.0).min(359.9), sv, v], None, None);
                    }
                }
                Ask::PkField(i, t) => self.pick_field(i, &t),
                Ask::PkWell(c) => {
                    let rgb = [c.red(), c.green(), c.blue()];
                    self.pick_set(rgb2hsv(rgb), Some(rgb), None);
                }
                Ask::PkEnter => {
                    self.close_picker();
                }
                Ask::PkDismiss => self.close_picker(),
                Ask::PreviewTick => {
                    changed = false;
                    if self.previewing && !self.page.streams() {
                        self.push_preview();
                    }
                }
                Ask::StreamTick => {
                    changed = false;
                    if self.previewing && self.page.dev.is_some() && self.page.streams() {
                        self.push_preview();
                    }
                }
                Ask::Profile(n) => self.choose_profile(n as u8),
                Ask::AutoSave => {
                    changed = false;
                    self.auto_save();
                    // a passing message goes after a while; a problem stays until the next
                    if self.status_at.is_some_and(|t| t.elapsed() > Duration::from_secs(6)) && self.ui.get_status_kind() != 2 {
                        self.status_at = None;
                        self.ui.set_status_text("".into());
                        self.ui.set_undo_shown(false);
                    }
                }
                Ask::Undo(redo) => self.undo(redo),
                Ask::Key(pos) => self.select_key(pos as u32),
                Ask::ShowRebindings => {
                    self.rebindings = !self.rebindings;
                    crate::uistate::set("show_rebindings", serde_json::json!(self.rebindings));
                    self.refresh();
                }
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
                Ask::Go(place) => {
                    self.close_menu();
                    self.ui.set_place(place.into());
                }
                // the item already chosen opens the rail into its blade, or closes it
                // the Key lab is a window of its own, not a place in this one
                Ask::Rail(place) if place == "lab" => {
                    self.close_menu();
                    window.push(WindowAsk::OpenLab);
                }
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
                // which profile the keyboard is using: it can be switched on the keyboard itself
                Ask::PollActive => {
                    changed = false;
                    if self.page.dev.is_some() && !self.active_asked && self.kb.is_some() {
                        self.active_asked = true;
                        self.ask_kb(device::Ask::ReadActive);
                    }
                }
                Ask::PollLcd => {
                    self.poll_lcd();
                    // Settings is kept current while it shows: a device can be
                    // plugged in, or the weather come in, at any time
                    changed = self.ui.get_place() == "settings";
                }
                Ask::MacroEditor(l) => {
                    if self.page.dev.is_none() {
                        continue;
                    }
                    self.close_menu();
                    let n = match (usize::try_from(l).ok().and_then(|l| LAYERS.get(l)), self.page.sel) {
                        (Some((layer, _)), Some(pos)) => Some(self.page.def(layer, pos)).filter(|d| d[0] == 3).map_or(0, |d| d[1]),
                        _ => 0,
                    };
                    window.push(WindowAsk::OpenMacros(n));
                }
            }
            any |= changed;
        }
        if any {
            self.refresh();
        }
        window
    }

    /// The keyboard's light as it lights it now: which caps are pale.
    pub fn light_caps(&mut self, light: &serde_json::Value) {
        let controls = crate::cores::shown().device["controls"].as_array().cloned().unwrap_or_default();
        self.pale = controls.iter().filter_map(|c| Some((c["id"].as_str()?, c["pos"].as_u64()? as u32))).filter(|(id, _)| ectodeck_draw::hdr::pale_top(light, id)).map(|(_, pos)| pos).collect();
    }

    /// Do what the Key lab asked; what only its window can do is handed back.
    /// (The picker's answer for a typeface, not a part's picture.)
    pub fn act_lab(&mut self) -> Vec<WindowAsk> {
        let Some(l) = &mut self.lab else { return Vec::new() };
        let window = l.act();
        // the light setup chosen to light the keyboard, kept for next time
        if let Some(setup) = l.switched.take() {
            let _ = self.settings.set(crate::setups::IN_USE, setup.map_or(serde_json::Value::Null, serde_json::Value::from));
        }
        // a picture asked for: the desktop's own file picker, and the file
        // chosen there put on the key, kept in Ectodeck's data
        if let Some(part) = l.image_wanted.take() {
            self.picking = Some((part, crate::picker::open_file("Choose a picture for the key", "SVG pictures", &["*.svg", "*.SVG"])));
        }
        // a typeface of the person's own asked for: the same picker, the file
        // chosen kept in Ectodeck's typefaces and the legends set in it
        if std::mem::take(&mut l.typeface_wanted) {
            self.picking = Some((TYPEFACE_PICK, crate::picker::open_file("Choose a typeface", "Typefaces", &["*.ttf", "*.otf", "*.TTF", "*.OTF"])));
        }
        if let Some((part, rx)) = &self.picking {
            let part = *part;
            match rx.try_recv() {
                Ok(chosen) => {
                    self.picking = None;
                    match (chosen, part) {
                        (Some(path), TYPEFACE_PICK) => match crate::typefaces::add(&path) {
                            Ok(families) => l.typeface_added(&families),
                            Err(why) => l.image_refused(&why),
                        },
                        (Some(path), _) => match crate::setups::import_image(&crate::cores::shown().id, &path) {
                            Ok(name) => l.put_image(part, &name),
                            Err(why) => l.image_refused(&why),
                        },
                        (None, _) => {}
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.picking = None,
            }
        }
        window
    }

    /// Do what the Macro Editor's page asked; what only its window can do is handed back.
    pub fn act_macros(&mut self) -> Vec<WindowAsk> {
        let Some(m) = &mut self.macros else { return Vec::new() };
        let kb = self.kb.clone();
        let tell = move |a: device::Ask| {
            if let Some(k) = &kb {
                let _ = k.send(a);
            }
        };
        let (window, changed) = m.act(&mut self.page, &tell);
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
            self.show_overlays();
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
        let fr = [self.overlays.fracs[0].unwrap_or(0.0), self.overlays.fracs[1].unwrap_or(0.0)];
        // the chosen slider's box follows it as it moves
        if self.slider_sel != 0 {
            self.ui.set_slider_at(fr[self.slider_sel - 1]);
        }
        self.drive_sliders(v, fr);
        if wider {
            crate::uistate::set("slider_ends", serde_json::json!(self.slider_ends));
        }
        self.show_overlays();
    }

    /// What is over the keyboard, changed in place, so a knob glides to its
    /// new place rather than being drawn anew there.
    pub fn show_overlays(&mut self) {
        let list = self.overlays.model();
        if self.overlay_model.row_count() == list.len() {
            for (i, o) in list.into_iter().enumerate() {
                self.overlay_model.set_row_data(i, o);
            }
        } else {
            self.overlay_model.set_vec(list);
        }
    }

    /// The keyboard's size on the page, in the buffer's pixels: the knobs and
    /// the display's glass are drawn again for a new one.
    pub fn board_size(&mut self, w: u32, h: u32, scale: f32) {
        if self.overlays.board(w, h, scale) {
            self.show_overlays();
        }
    }

    // ------------------------------------------------------------ the Lighting page

    /// The keyboard shows the draft while the Lighting page is open. Nothing is
    /// stored, so leaving without saving gives the profile back untouched.
    fn start_preview(&mut self) {
        self.previewing = true;
        self.push_preview();
    }

    /// The window closing or Ectodeck quitting: the keyboard goes back to what its
    /// profile stores, its lighting and its screen, whatever is left unsaved.
    pub fn hand_back(&mut self) {
        self.stop_preview();
        if let Some(d) = &self.page.dev {
            let saved = screen_of(&d["profiles"][self.page.active().to_string()]["lighting"]);
            if self.screen_sent != Some(saved) {
                self.screen_sent = Some(saved);
                self.ask_kb(device::Ask::Screen(saved.0, saved.1));
            }
        }
    }

    pub fn stop_preview(&mut self) {
        if self.previewing {
            self.previewing = false;
            self.ui.set_live(false);
            self.ask_kb(device::Ask::Preview(serde_json::json!({ "stop": true, "tab": "native", "claim": true })));
        }
    }

    fn push_preview(&mut self) {
        if !self.previewing || self.page.dev.is_none() || self.preview_asked || self.kb.is_none() {
            return;
        }
        let mut body = self.page.preview_body();
        body["tab"] = serde_json::json!("native");
        body["claim"] = serde_json::json!(true);
        self.preview_asked = true;
        self.ask_kb(device::Ask::Preview(body));
    }

    /// A piece of the tile used: a swatch opens the colour picker, the rest change the draft.
    fn cell_pick(&mut self, c: Ctl, index: usize, _anchor: (f32, f32, f32)) {
        match c {
            Ctl::Color | Ctl::Colors | Ctl::Background | Ctl::Paint | Ctl::LcdColor => {
                self.open_picker(c, index, _anchor);
                return;
            }
            Ctl::Start | Ctl::Stop | Ctl::Submode | Ctl::Plays => {
                self.open_light_menu(c, _anchor);
                return;
            }
            Ctl::WhereMode | Ctl::WhereAll | Ctl::Zone => {
                let mode = self.page.draft()["lighting"]["mode"].as_str().unwrap_or("off").to_string();
                let areas = self.areas.entry((self.page.editing, mode)).or_insert(lighting::ALL_AREAS);
                lighting::place(&mut self.page, c, index, areas);
            }
            Ctl::Preset => {
                if let Some(c) = lighting::PRESETS.get(index) {
                    self.paint = *c;
                }
            }
            // a checkbox: which box, and the state it is turned to
            Ctl::Demo => self.page.demo = index % 2 == 1,
            Ctl::Random => lighting::act(&mut self.page, c, 0, (index % 2) as f32),
            Ctl::LcdApp | Ctl::GameKey => crate::display::act(&mut self.page, c, index / 2, (index % 2) as f32, None),
            Ctl::LcdPreset => crate::display::act(&mut self.page, c, index, 0.0, None),
            Ctl::SliderBind => {
                self.open_bind_menu(index, _anchor);
                return;
            }
            Ctl::Control | Ctl::Temperature | Ctl::Wind | Ctl::HdrMode => {
                let Some(v) = crate::ecto_settings::choice(c, index) else { return };
                let path = match c {
                    Ctl::Control => "devices.z10.control",
                    Ctl::Temperature => "weather.units.temperature",
                    Ctl::Wind => "weather.units.wind",
                    _ => "hdr.lighting",
                };
                if self.settings.get(path).as_ref() == Some(&v) || (c == Ctl::Control && v == serde_json::json!(self.controlled)) {
                    return;
                }
                if c == Ctl::Control && v == serde_json::json!(false) {
                    // give the keyboard back what Ectodeck borrowed before letting it go
                    self.ask_kb(device::Ask::Modes(false, false));
                    self.ask_kb(device::Ask::Dimmer(None));
                    self.stop_preview();
                }
                if let Err(e) = self.settings.set(path, v) {
                    self.status(format!("The setting was not saved: {e}"), 2);
                    return;
                }
                // a change to what Ectodeck may use starts it again, back on Settings
                if c == Ctl::Control {
                    self.restart = Some("settings".into());
                }
            }
            Ctl::Fill | Ctl::Scatter | Ctl::Clear => {
                // Fill paints an area (the whole keyboard, or one of the others); the rest every key
                let area = if c == Ctl::Fill { lighting::ZONES.get(index).map_or(0x80, |z| z.0) as u8 } else { 0x80 };
                let within = self.page.zone_set(area);
                let leds: Vec<u32> = self.page.layout.iter().filter(|k| k.led && within.as_ref().is_none_or(|w| w.contains(&k.pos))).map(|k| k.pos).collect();
                let mut seed = self.t0.elapsed().as_nanos() as u64 | 1;
                for pos in 0..self.page.custom.len() as u32 {
                    let led = leds.contains(&pos);
                    self.page.custom[pos as usize] = match c {
                        Ctl::Fill if led => self.paint,
                        Ctl::Scatter if led => [0, 1, 2].map(|_| {
                            seed ^= seed << 13;
                            seed ^= seed >> 7;
                            seed ^= seed << 17;
                            (seed >> 56) as u8
                        }),
                        Ctl::Clear => [0; 3],
                        _ => self.page.custom[pos as usize],
                    };
                }
                lighting::save_custom(self.page.editing, &self.page.custom);
            }
            _ => lighting::act(&mut self.page, c, index, 0.0),
        }
        self.push_preview();
    }

    // ------------------------------------------------------------ the colour picker

    fn open_picker(&mut self, c: Ctl, index: usize, anchor: (f32, f32, f32)) {
        // the second click on the swatch whose picker a press just closed
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        self.close_menu();
        self.close_picker();
        let rgb = lighting::color_of(&self.page, c, index, self.paint);
        self.pick = Some(Pick { ctl: c, index, start: rgb, hsv: rgb2hsv(rgb), rgb, anchor });
        let recent = recent_colors();
        self.ui.set_pk_recent(ModelRc::new(VecModel::from(recent.iter().map(|c| color(*c)).collect::<Vec<_>>())));
        // under the swatch when it fits there, or when there is a fair amount of room there anyway; over it when below is cramped
        let (ax, ay, ah) = anchor;
        let (h, win) = (self.ui.get_picker_height(), self.ui.get_window_height());
        let (below, above) = (win - ay - ah - 14.0, ay - 70.0);
        let down = h <= below || below >= h.min(260.0) || below >= above;
        let room = if down { below } else { above };
        self.ui.set_picker_x(ax);
        self.ui.set_picker_top(if down { ay + ah + 6.0 } else { ay - 6.0 - h.min(room) });
        self.ui.set_pk_hue_img(hue_bar(self.scale));
        self.show_pick(None);
        self.ui.set_picker_open(true);
    }

    fn close_picker(&mut self) {
        if let Some(p) = self.pick.take() {
            self.ui.set_picker_open(false);
            self.ui.invoke_blur_picker();
            // the colour the picker is closed on joins the recent ones
            if p.rgb != p.start {
                keep_recent(p.rgb);
            }
        }
    }

    /// Show the colour everywhere but the field being typed in.
    fn show_pick(&mut self, skip: Option<i32>) {
        let Some(p) = &self.pick else { return };
        let [h, sv, v] = p.hsv;
        let rgb = p.rgb;
        self.ui.set_pk_hue(h);
        self.ui.set_pk_sat(sv);
        self.ui.set_pk_val(v);
        self.ui.set_pk_now(color(rgb));
        self.ui.set_pk_pure(color(hsv2rgb([h, 1.0, 1.0])));
        self.ui.set_pk_sv_img(sv_square(h, self.scale));
        let fields = [
            format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
            rgb[0].to_string(),
            rgb[1].to_string(),
            rgb[2].to_string(),
            (h.round() as i32).to_string(),
            ((sv * 100.0).round() as i32).to_string(),
            ((v * 100.0).round() as i32).to_string(),
        ];
        let setters: [fn(&AppWindow, SharedString); 7] = [
            AppWindow::set_pk_hex,
            AppWindow::set_pk_r,
            AppWindow::set_pk_g,
            AppWindow::set_pk_b,
            AppWindow::set_pk_h,
            AppWindow::set_pk_s,
            AppWindow::set_pk_v,
        ];
        for (i, (set, t)) in setters.iter().zip(fields).enumerate() {
            if skip != Some(i as i32) {
                set(&self.ui, t.into());
            }
        }
    }

    /// A new colour in the picker: shown, and set where the picker was opened.
    fn pick_set(&mut self, hsv: [f32; 3], rgb: Option<[u8; 3]>, skip: Option<i32>) {
        let Some(p) = &mut self.pick else { return };
        p.hsv = hsv;
        p.rgb = rgb.unwrap_or_else(|| hsv2rgb(hsv));
        let (c, index, rgb) = (p.ctl, p.index, p.rgb);
        if c == Ctl::Paint {
            self.paint = rgb;
        } else {
            lighting::set_color(&mut self.page, c, index, rgb);
        }
        self.show_pick(skip);
        self.push_preview();
    }

    /// A field typed in: a hex code, or a number of its own.
    fn pick_field(&mut self, i: i32, t: &str) {
        let Some(p) = &self.pick else { return };
        let (rgb, hsv) = (p.rgb, p.hsv);
        match i {
            0 => {
                let t = t.trim().trim_start_matches('#');
                let full: String = if t.len() == 3 { t.chars().flat_map(|c| [c, c]).collect() } else { t.to_string() };
                if full.len() == 6 {
                    if let Ok(v) = u32::from_str_radix(&full, 16) {
                        let c = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                        self.pick_set(rgb2hsv(c), Some(c), Some(0));
                    }
                }
            }
            1..=3 => {
                if let Ok(v) = t.trim().parse::<f32>() {
                    let mut c = rgb;
                    c[(i - 1) as usize] = v.round().clamp(0.0, 255.0) as u8;
                    self.pick_set(rgb2hsv(c), Some(c), Some(i));
                }
            }
            4..=6 => {
                if let Ok(v) = t.trim().parse::<f32>() {
                    let mut n = hsv;
                    match i {
                        4 => n[0] = v.clamp(0.0, 359.0),
                        5 => n[1] = v.clamp(0.0, 100.0) / 100.0,
                        _ => n[2] = v.clamp(0.0, 100.0) / 100.0,
                    }
                    self.pick_set(n, None, Some(i));
                }
            }
            _ => {}
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

    /// Notice what changed since last looked: each change marks its profile
    /// to be saved, and what it was before can be undone. Changes close
    /// together (a slider dragged, a color picked) undo as one.
    fn note_changes(&mut self) {
        if self.page.dev.is_none() {
            return;
        }
        let now = Instant::now();
        for (n, d) in &self.page.drafts {
            match self.last_drafts.get(n) {
                None => {
                    self.last_drafts.insert(*n, d.clone());
                }
                Some(before) if before != d => {
                    let burst = self.undo_mark.is_some_and(|(m, t)| m == *n && t.elapsed() < Duration::from_millis(1200));
                    if !burst {
                        self.undo.push((*n, before.clone()));
                        if self.undo.len() > 100 {
                            self.undo.remove(0);
                        }
                    }
                    self.undo_mark = Some((*n, now));
                    self.redo.clear();
                    self.changed_at.insert(*n, now);
                    self.last_drafts.insert(*n, d.clone());
                }
                _ => {}
            }
        }
    }

    /// Save a profile that has changed, once it has been still a moment.
    fn auto_save(&mut self) {
        self.note_changes();
        if self.page.dev.is_none() || self.page.busy || self.kb.is_none() {
            return;
        }
        let quiet = |t: Option<&Instant>| t.is_some_and(|t| t.elapsed() >= Duration::from_millis(1200));
        let due = (1..=5u8).find(|n| self.page.is_dirty(*n) && quiet(self.changed_at.get(n)));
        if let Some(n) = due {
            self.save_now(n);
        }
    }

    fn save_now(&mut self, n: u8) {
        let Some(d) = self.page.drafts.get(&n).cloned() else { return };
        self.page.busy = true;
        self.saving = Some((n, d));
        self.ask_kb(device::Ask::Save(n, self.page.save_body_of(n)));
    }

    /// Put back how a profile was before its last change (or, `redo`, as it
    /// was before the last undo); it saves as any change does.
    fn undo(&mut self, redo: bool) {
        self.note_changes();
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some((n, d)) = from.pop() else { return };
        if let Some(now) = self.page.drafts.insert(n, d.clone()) {
            to.push((n, now));
        }
        self.last_drafts.insert(n, d);
        self.changed_at.insert(n, Instant::now());
        self.undo_mark = None;
        let name = self.page.profile_name(n);
        self.status(if redo { format!("Did again the change to {name}") } else { format!("Undid the last change to {name}") }, 0);
        self.push_preview();
        self.refresh();
    }

    /// Ectodeck closing: whatever has changed is saved now, without waiting.
    /// Whether everything is saved, so it may go.
    pub fn flush(&mut self) -> bool {
        self.note_changes();
        if self.released.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
            return false;
        }
        if self.page.dev.is_none() || self.kb.is_none() {
            return true;
        }
        if !self.page.busy {
            if let Some(n) = (1..=5u8).find(|n| self.page.is_dirty(*n) && self.changed_at.contains_key(n)) {
                self.save_now(n);
            }
        }
        !self.page.busy && !(1..=5u8).any(|n| self.page.is_dirty(n) && self.changed_at.contains_key(&n))
    }

    /// What the page's typing goes to: a box being typed in takes Ctrl+Z itself.
    pub fn typing(&self) -> bool {
        slint::ComponentHandle::global::<crate::Typing>(&self.ui).get_on()
    }

    /// A key clicked on the board: chosen for editing on the Keys page; on
    /// the Lighting page it lights as pressing it would.
    fn select_key(&mut self, pos: u32) {
        self.slider_sel = 0;
        if self.page.dev.is_none() {
            return;
        }
        let now = self.t0.elapsed().as_secs_f64() * 1000.0;
        if self.page.view == View::Lighting {
            // Per key: a click paints the key
            if self.page.draft()["lighting"]["mode"] == "custom" {
                let Some(k) = self.page.key(pos).cloned() else { return };
                if !k.led {
                    self.status(format!("{} has no light behind it.", self.page.key_label(&k)), 0);
                    return;
                }
                if let Some(c) = self.page.custom.get_mut(pos as usize) {
                    *c = self.paint;
                }
                lighting::save_custom(self.page.editing, &self.page.custom);
                self.push_preview();
                return;
            }
            // on Some areas, a key in an area turns that area on or off, as its chip does
            if self.some_areas() {
                if let Some(i) = self.area_of(pos) {
                    self.cell_pick(Ctl::Zone, i, (0.0, 0.0, 0.0));
                    return;
                }
            }
            // otherwise it lights as pressing it would, for Reactive
            self.page.flash(pos, now);
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
    }


    /// A key typed into Ectodeck's window, as a Linux key code, which "Press
    /// a key" takes. True if it was taken. (The board shows the keyboard's
    /// keys going down and coming up from the keyboard itself: see [watched].)
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
        false
    }

    /// What the keyboard did, from its own devices, whichever window has it:
    /// a key on the board goes down and comes up with the real one (lighting
    /// its Reactive effect as it goes down), the lock lights follow the
    /// keyboard's, and a keyboard gone lets every key go.
    pub fn watched(&mut self, w: Watched) {
        match w {
            Watched::Key(code, down) => {
                let Some(pos) = self.page.pos_for(code as u32).filter(|p| self.page.key(*p).is_some()) else { return };
                if down {
                    if self.page.pressed.insert(pos) {
                        let now = self.t0.elapsed().as_secs_f64() * 1000.0;
                        self.page.flash(pos, now);
                        self.refresh();
                    }
                } else if self.page.pressed.remove(&pos) {
                    self.refresh();
                }
            }
            Watched::Locks(bits) => {
                if self.page.locks != bits {
                    self.page.locks = bits;
                    // the keyboard is lit again with them
                    self.last_lights = None;
                }
            }
            Watched::Gone => {
                if !self.page.pressed.is_empty() {
                    self.page.pressed.clear();
                    self.refresh();
                }
            }
        }
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
        // the job's own kind of list, or the whole menu for a key that does nothing
        let page = self.page.sel.map(|pos| self.page.def(LAYERS[layer].0, pos)).and_then(|d| match z10::kind_of(d) {
            "Key" => Some(0),
            "Media" => Some(1),
            "Mouse" => Some(2),
            "Macro" => Some(MACRO_PAGE),
            "Profile" => Some(3),
            "Keyboard" => Some(4),
            _ => None,
        });
        self.open_menu_on(layer, anchor, page);
    }

    fn open_menu_on(&mut self, layer: usize, anchor: (f32, f32, f32), page: Option<usize>) {
        let Some(pos) = self.page.sel else { return };
        // the second click on the button whose menu a press just closed
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        if self.menu.as_ref().is_some_and(|m| m.layer == layer && m.page == page) {
            self.close_menu();
            return;
        }
        self.listening = None;
        self.menu = Some(Menu { layer, pos, page, query: String::new(), anchor, choices: Vec::new() });
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
        if self.bind_menu.take().is_some() || self.light_menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_menu_w(300.0);
        }
        if self.menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.invoke_blur_menu();
        }
    }

    /// A press somewhere in the window, in its logical pixels: a press away
    /// from the open menu closes it, and a press on the button it hangs from
    /// keeps that button's click from opening it again.
    pub fn pressed_at(&mut self, x: f32, y: f32) {
        self.let_key_go(x, y);
        if self.place_menu {
            let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
            if !(x >= mx && x < mx + mw && y >= my && y < my + mh) {
                self.close_place_menu();
            }
        }
        if self.bind_menu.is_some() || self.light_menu.is_some() {
            let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
            if !(x >= mx && x < mx + mw && y >= my && y < my + mh) {
                // a press on the dropdown the menu hangs from shuts it, and its click must not open it again
                let (ax, ay, ah) = (self.ui.get_menu_ax(), self.ui.get_menu_ay(), self.ui.get_menu_ah());
                if self.light_menu.is_some() && x >= ax && x < ax + self.ui.get_menu_w() && y >= ay && y < ay + ah {
                    self.shut = Some(((ax, ay), Instant::now()));
                }
                self.close_menu();
                self.refresh();
            }
            return;
        }
        // away from the open picker closes it; on the swatch it hangs from too
        if self.pick.is_some() {
            let (px, py, ph) = (self.ui.get_picker_x().max(8.0).min(self.ui.get_window_width() - 272.0), self.ui.get_picker_top(), self.ui.get_picker_height());
            if !(x >= px && x < px + 264.0 && y >= py && y < py + ph) {
                let (ax, ay, ah) = self.pick.as_ref().unwrap().anchor;
                if x >= ax && x < ax + 34.0 && y >= ay && y < ay + ah {
                    self.shut = Some(((ax, ay), Instant::now()));
                }
                self.close_picker();
            }
        }
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

    /// A press anywhere on the page lets the chosen key go, but on the
    /// keyboard (a key there is chosen instead, and the chassis lets it go
    /// itself), on the chosen key's editor, in an open menu or the colour
    /// picker, or on the title bar, which only moves the window.
    fn let_key_go(&mut self, x: f32, y: f32) {
        if self.page.view != View::Keys || self.page.sel.is_none() || self.ui.get_place() != "z10" || y < 36.0 {
            return;
        }
        if self.menu.is_some() || self.bind_menu.is_some() || self.light_menu.is_some() || self.pick.is_some() || self.place_menu {
            return;
        }
        let inside = |x0: f32, y0: f32, w: f32, h: f32| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
        let ui = &self.ui;
        let board = ui.get_board_shown() && inside(ui.get_board_x(), ui.get_board_y(), ui.get_board_width(), ui.get_board_height());
        let editor = ui.get_key_open() && inside(ui.get_key_tile_x(), ui.get_key_tile_y(), ui.get_key_tile_w(), ui.get_key_tile_h());
        if board || editor {
            return;
        }
        self.page.sel = None;
        self.listening = None;
        self.close_menu();
        self.refresh();
    }

    fn menu_chose(&mut self, i: usize) {
        if self.place_menu {
            self.close_place_menu();
            if let Some(p) = self.found.get(i).cloned() {
                self.place_text = p.name.clone();
                if let Ok(v) = serde_json::to_value(&p) {
                    let _ = self.settings.set("weather.place", v);
                }
                self.weather.set_place(weather_place(&self.settings));
                self.refresh();
            }
            return;
        }
        if let Some(c) = self.light_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_menu_w(300.0);
            let index = if c == Ctl::Submode { i.min(2) } else { i };
            // Plays: its rows, a rule between the two halves (and, for a pair it
            // has no name for, that one after another rule, which changes nothing)
            let index = if c == Ctl::Plays {
                match plays_rows(&self.page).get(i).filter(|r| r.kind == 0).and_then(|r| lighting::PLAYS.iter().position(|p| p.0 == r.label.as_str())) {
                    Some(k) => k,
                    None => {
                        self.refresh();
                        return;
                    }
                }
            } else {
                index
            };
            lighting::act(&mut self.page, c, index, 0.0);
            self.push_preview();
            self.refresh();
            return;
        }
        if let Some((s, _)) = self.bind_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_menu_w(300.0);
            if let Some((v, _)) = i.checked_sub(1).and_then(|k| crate::display::BIND_CHOICES[s].get(k)) {
                self.set_slider_bind(s, v);
            }
            return;
        }
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
        let row = |kind: i32, label: &str, tag: &str, tick: bool| MenuRow { kind, label: label.into(), tag: tag.into(), tick, sub: "".into() };
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
            Some(MACRO_PAGE) => {
                placeholder = "Search macros".to_string();
                if g.macros.is_empty() {
                    rows.push(row(3, "No macros yet. Make one in the Macro Editor.", "", false));
                    choices.push(Choice::Nothing);
                } else {
                    pick(&mut rows, &mut choices, &[("Macros", g.macros.iter().collect())]);
                }
            }
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
        self.note_changes();
        let ui = &self.ui;
        let p = &self.page;
        ui.set_tab(match p.view {
            View::Keys => 0,
            View::Lighting => 1,
            View::Display => 2,
            View::Screen => 3,
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
                        unsaved: 0,
                        colors,
                        c1,
                        c2,
                        c3,
                        c4,
                    }
                })
                .collect();
            show_rows(ui.get_profiles(), |m| ui.set_profiles(m), profiles);
        }
        ui.set_layer(self.layer as i32);
        if loaded {
        }
        ui.set_hint(
            match p.view {
                View::Keys if p.sel.is_some() => "Escape clears the choice.",
                View::Keys => "Click a key on the keyboard or in the Key library to change what the key does.",
                View::Lighting => "Choose a lighting effect for this profile, then set the colors, speed and zones below.",
                View::Display => "The keyboard’s settings are the same on every profile.",
                View::Screen => "Choose what the keyboard’s screen shows while Ectodeck is open and when Ectodeck is closed.",
            }
            .into(),
        );
        // on Screen, what the screen is doing now, beside Pause
        ui.set_screen_status(self.screen_status().into());

        // the keys over the keyboard; on Lighting, the area the pointer is over outlined
        let (x0, y0, _, _) = p.view_box;
        let zoned = self.area_keys();
        let some = self.some_areas();
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
                remap: p.remap(k.pos, &names).filter(|_| self.rebindings).map(|s| s.to_uppercase()).unwrap_or_default().into(),
                pale: self.pale.contains(&k.pos),
                selected: p.view == View::Keys && p.sel == Some(k.pos),
                pressed: p.pressed.contains(&k.pos),
                zoned: zoned.as_ref().is_some_and(|z| z.contains(&k.pos)),
                // the hand only where a click does something: choosing a key, painting
                // one, or on Some areas, turning its area on or off
                clickable: match p.view {
                    View::Keys => true,
                    View::Lighting if p.drafts.get(&p.editing).is_some_and(|d| d["lighting"]["mode"] == "custom") => k.led,
                    View::Lighting => some && self.area_of(k.pos).is_some(),
                    _ => false,
                },
            })
            .collect();
        show_rows(ui.get_keys(), |m| ui.set_keys(m), keys);
        ui.set_dimming(zoned.is_some());
        ui.set_show_rebindings_on(self.rebindings);

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
            show_rows(ui.get_layers(), |m| ui.set_layers(m), layers);
            // the one editor, on the layer shown
            let layer = LAYERS[self.layer].0;
            let d = p.def(layer, k.pos);
            let rebound_here = d != p.factory(layer, k.pos);
            let rebound = rebound_here || p.def(LAYERS[1 - self.layer].0, k.pos) != p.factory(LAYERS[1 - self.layer].0, k.pos);
            let fn_unset = self.layer == 1 && d == p.def("keys", k.pos);
            let job = if fn_unset { "Same as on Base".to_string() } else if d[0] == 3 { p.macro_label(d[1]) } else if z10::is_nothing(d) { "Nothing".into() } else { p.describe(d) };
            ui.set_key_cap(lib_cap(&k.id, &name).into());
            ui.set_key_state((if rebound { format!("Rebound on {}.", p.profile_name(p.editing)) } else { "As it came from the factory.".to_string() }).into());
            ui.set_key_rebound(rebound);
            ui.set_layer_rebound(rebound_here);
            ui.set_key_kind(match z10::kind_of(d) {
                "Key" => 0,
                "Media" => 1,
                "Mouse" => 2,
                "Macro" => 3,
                "Profile" => 4,
                "Keyboard" => 5,
                _ => 6,
            });
            ui.set_key_job(job.into());
            ui.set_key_listening(self.listening.is_some_and(|(l, _)| l == self.layer));
            ui.set_key_note((if fn_unset { "On the Function layer this key does what it does on Base, until you choose a job here." } else { "" }).into());
        }
        // the Key library: each group, open or closed, its keys as they are on the layer shown
        if loaded && p.view == View::Keys {
            // something newly chosen: its group opens, once
            let chosen = match self.slider_sel {
                1 => Some(-1),
                2 => Some(-2),
                _ => p.sel.map(|pos| pos as i32),
            };
            let newly = chosen != self.lib_revealed;
            if newly {
                self.lib_revealed = chosen;
                let group = match chosen {
                    Some(-1) | Some(-2) => Some("Sliders"),
                    Some(pos) => p.key(pos as u32).map(|k| lib_group_of(&k.id)),
                    None => None,
                };
                if let Some(g) = group {
                    self.lib_shut.remove(g);
                }
            }
            let layer = LAYERS[self.layer].0;
            let q = self.lib_query.trim().to_lowercase();
            let mut groups: Vec<LibGroup> = Vec::new();
            // the sliders first, as keys are listed
            let bind_label = |i: usize| crate::display::BIND_CHOICES[i].iter().find(|(v, _)| *v == self.bind[i]).map_or("", |c| c.1).to_string();
            let mut slider_rows = vec![
                LibRow { id: -1, cap: "◐".into(), name: "Dimmer".into(), job: bind_label(0).into(), rebound: false, chosen: self.slider_sel == 1 },
                LibRow { id: -2, cap: "♪".into(), name: "Volume".into(), job: bind_label(1).into(), rebound: false, chosen: self.slider_sel == 2 },
            ];
            slider_rows.retain(|r| q.is_empty() || r.name.to_lowercase().contains(&q) || r.job.to_lowercase().contains(&q));
            // each group's rows keep one list, changed in place, so a click on a
            // heading or a row is never lost to a refresh between press and release
            let lib_rows = &mut self.lib_rows;
            let mut push = |name: &str, rows: Vec<LibRow>, total: usize| {
                if rows.is_empty() && !q.is_empty() {
                    return;
                }
                let open = !q.is_empty() || !self.lib_shut.contains(name);
                let rebound = rows.iter().filter(|r| r.rebound).count() as i32;
                let model = lib_rows.entry(name.to_string()).or_insert_with(|| Rc::new(VecModel::default())).clone();
                set_rows(&model, if open { rows } else { Vec::new() });
                groups.push(LibGroup { name: name.into(), count: total as i32, rebound, open, rows: ModelRc::from(model) });
            };
            // (on Base only: Fn gives the sliders no second job)
            if self.layer == 0 {
                push("Sliders", slider_rows, 2);
            }
            for (g, keys) in lib_groups(p) {
                let total = keys.len();
                let rows: Vec<LibRow> = keys
                    .iter()
                    .map(|k| {
                        let name = p.key_label(k);
                        let d = p.def(layer, k.pos);
                        let fac = p.factory(layer, k.pos);
                        let re = d != fac;
                        let job = if re {
                            if d[0] == 3 { p.macro_label(d[1]) } else if z10::is_nothing(d) { "Nothing".into() } else { p.describe(d) }
                        } else if g == "Buttons" && !z10::is_nothing(fac) && p.describe(fac) != name {
                            p.describe(fac)
                        } else {
                            String::new()
                        };
                        LibRow { id: k.pos as i32, cap: lib_cap(&k.id, &name).into(), name: name.clone().into(), job: job.into(), rebound: re, chosen: p.sel == Some(k.pos) }
                    })
                    .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q) || r.job.to_lowercase().contains(&q))
                    .collect();
                push(&g, rows, total);
            }
            // something newly chosen: the list scrolls to its row (a heading 30 high, a row 34)
            if newly && chosen.is_some() {
                let mut y = 0.0;
                for g in &groups {
                    y += 30.0;
                    let rows: Vec<LibRow> = (0..g.rows.row_count()).filter_map(|i| g.rows.row_data(i)).collect();
                    if let Some(i) = rows.iter().position(|r| r.chosen) {
                        ui.set_lib_reveal_y(y + i as f32 * 34.0);
                        ui.set_lib_reveal_n(ui.get_lib_reveal_n() + 1);
                        break;
                    }
                    y += rows.len() as f32 * 34.0;
                }
            }
            show_rows(ui.get_lib_groups(), |m| ui.set_lib_groups(m), groups);
            // Game mode's keys, blocked or not
            let blocked = !p.draft()["lighting"]["game_keys"].as_u64().unwrap_or(0) & 0x7f;
            let items: Vec<CheckItem> = crate::display::GAME_KEYS.iter().map(|(bit, n)| CheckItem { name: (*n).into(), on: blocked & bit != 0 }).collect();
            show_rows(ui.get_game_items(), |m| ui.set_game_items(m), items);
            // a slider chosen: where it sits, what it controls
            ui.set_slider_chosen(self.slider_sel as i32);
            if self.slider_sel != 0 {
                let s = self.slider_sel - 1;
                ui.set_slider_at(self.overlays.fracs[s].unwrap_or(0.0));
                let choices: Vec<CheckItem> = crate::display::BIND_CHOICES[s].iter().map(|(v, l)| CheckItem { name: (*l).into(), on: self.bind[s] == *v }).collect();
                show_rows(ui.get_slider_choices(), |m| ui.set_slider_choices(m), choices);
                // what the firmware can't do yet, said where it matters
                let fw = self.firmware();
                let resting = self.idled[s].as_ref().filter(|_| self.bind[s] == "none").and_then(|job| crate::display::BIND_CHOICES[s].iter().find(|(v, _)| v == job)).map(|(_, l)| *l);
                let other = if s == 0 { "volume slider" } else { "dimmer" };
                let rest_note = resting.map(|l| format!("Set to Nothing while the {other} does {}. It goes back to {} when the {other} does something else.", l.to_lowercase(), l.to_lowercase()));
                let note = if let Some(n) = &rest_note {
                    n.as_str()
                } else if s == 1 && self.bind[1] != "own" && !crate::display::has(&fw, "volume.quiet") {
                    "The keyboard still sends volume keys from this slider as well. Its firmware can be told not to, from build 3."
                } else if s == 0 && self.bind[0] != "own" {
                    if crate::display::has(&fw, "dimmer.keep") {
                        "The backlight stays where it was, and follows the volume slider if that is on keyboard brightness."
                    } else {
                        "The backlight stays where it was, though it may flicker as this slider moves. Its firmware can hold it steady, from build 3."
                    }
                } else {
                    ""
                };
                ui.set_slider_note(note.into());
            }
        }
        if self.menu.is_some() {
            self.build_menu();
        }
        if loaded && self.page.view == View::Lighting && self.ui.get_place() == "z10" {
            self.refresh_lighting();
        }
        if self.ui.get_place() == "settings" {
            // what the kernel says of the keyboard, looked at every two seconds (it
            // scans every process), not at every step of a slider
            if self.dev_status.as_ref().is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(2)) {
                self.dev_status = Some((Instant::now(), crate::devices::z10()));
            }
            let dc = self.dev_status.as_ref().map(|(_, v)| v.clone()).unwrap_or_default();
            let st = crate::ecto_settings::State {
                status: dc,
                control: self.controlled,
                settings: self.settings.all(),
                report: self.weather.report(),
                locale: &crate::locale(),
                top_peak: self.levels.1,
                reference: self.levels.0,
                place_text: &self.place_text,
            };
            let (cells, h) = crate::ecto_settings::page(&st);
            self.ui.set_settings_h(h);
            self.show_cells(cells);
        } else if loaded && self.page.view == View::Screen {
            let t = crate::display::closed(&self.page, 320.0);
            self.ui.set_closed_h(t.height);
            self.show_cells(t.cells);
            self.draw_screen();
        } else if loaded && self.page.view == View::Display {
            let fw = self.firmware();
            let connected = match self.ui.get_conn_state() {
                1 => Some(true),
                3 => None,
                _ => Some(false),
            };
            let t = crate::display::tiles(&fw, connected);
            self.ui.set_tile_height(t.height);
            self.show_cells(t.cells);
        }
        // the screen as the profile being edited has it, on the keyboard now
        if loaded && self.controlled {
            let want = screen_of(&self.page.draft()["lighting"]);
            if self.screen_sent != Some(want) {
                self.screen_sent = Some(want);
                self.ask_kb(device::Ask::Screen(want.0, want.1));
            }
        }
        let back = self.page.lcd_color();
        if back != self.overlays.lcd.2 {
            self.overlays.lcd.2 = back;
            self.show_overlays();
        }        // the Macro Editor shows the same keyboard: its macros, and the keys they are on
        if let Some(m) = &mut self.macros {
            m.refresh(&self.page);
        }
    }

    /// The Lighting page's effects and its tile, laid out for the draft. The
    /// pieces are changed in place, so one being used is never rebuilt under
    /// the pointer.
    fn refresh_lighting(&mut self) {
        let mode = self.page.draft()["lighting"]["mode"].as_str().unwrap_or("off").to_string();
        let scale = self.scale;
        if self.icons.as_ref().map(|(s, _)| *s) != Some(scale) {
            self.icons = Some((scale, z10::EFFECTS.iter().map(|(m, _)| lighting::icon(m, scale)).collect()));
        }
        let icons = &self.icons.as_ref().unwrap().1;
        let effects: Vec<EffectItem> = z10::EFFECTS
            .iter()
            .zip(icons)
            .map(|((m, name), icon)| EffectItem {
                name: (*name).into(),
                icon: icon.clone(),
                on: *m == mode,
                motion: match *m {
                    "breathing" => 1,
                    "pulse" => 2,
                    "wave" => 3,
                    "stars" => 4,
                    "trigger" => 5,
                    _ => 0,
                },
            })
            .collect();
        self.ui.set_effect_moves(effects.iter().any(|e| e.on && e.motion != 0));
        show_rows(self.ui.get_effects(), |m| self.ui.set_effects(m), effects);
        let open = self.pick.as_ref().map(|p| (p.ctl, p.index));
        let b = lighting::boxes(&self.page, self.paint, self.page.demo, open, self.light_menu, self.picture_area());
        self.ui.set_light_kind(b.kind);
        self.ui.set_light_title(b.title.into());
        self.ui.set_set_h(b.set_h);
        self.ui.set_place_h(b.place_h);
        self.show_cells(b.set);
        sync_cells(&self.place_cells, b.place);
    }

    /// Whether the Lighting page lights some areas of the keyboard, not all of it.
    fn some_areas(&self) -> bool {
        let p = &self.page;
        // (nothing to say before the keyboard's profiles are read)
        let Some(d) = p.drafts.get(&p.editing) else { return false };
        let l = &d["lighting"];
        let mode = l["mode"].as_str().unwrap_or("off");
        p.view == View::Lighting && mode != "off" && mode != "custom" && l[mode]["zone"].as_u64().unwrap_or(0x80) & 0x80 == 0
    }

    /// The area a key is in, by its place in ZONES.
    fn area_of(&self, pos: u32) -> Option<usize> {
        (1..lighting::ZONES.len()).find(|&i| self.page.zone_set(lighting::ZONES[i].0 as u8).is_some_and(|keys| keys.contains(&pos)))
    }

    /// On Some areas, the area the pointer is over on the keyboard's picture.
    fn picture_area(&self) -> Option<usize> {
        if !self.some_areas() {
            return None;
        }
        self.area_of(self.picture_key?)
    }

    /// The keys of the area the pointer is over on the Lighting page, if it is
    /// over one: its chip, or the area itself on the keyboard's picture.
    fn area_keys(&self) -> Option<Vec<u32>> {
        let p = &self.page;
        if p.view != View::Lighting {
            return None;
        }
        let bit = lighting::ZONES.get(self.hover_area.or_else(|| self.picture_area())?)?.0 as u8;
        Some(p.zone_set(bit).unwrap_or_else(|| p.layout.iter().filter(|k| k.led).map(|k| k.pos).collect()))
    }

    /// Where the keys of the area the pointer is over are, in the keyboard's
    /// own pixels `w` across: the rest of the picture is dimmed around them.
    pub fn dim_rects(&self, w: u32) -> Option<Vec<[f32; 4]>> {
        let keys = self.area_keys()?;
        let (x0, y0, vw, _) = self.page.view_box;
        let s = w as f64 / vw;
        Some(
            self.page
                .layout
                .iter()
                .filter(|k| keys.contains(&k.pos))
                .map(|k| [((k.x - x0 + z10::GAP / 2.0) * s) as f32, ((k.y - y0 + z10::GAP / 2.0) * s) as f32, ((k.w - z10::GAP) * s) as f32, ((k.h - z10::GAP) * s) as f32])
                .collect(),
        )
    }

    /// The tile's pieces, changed in place where they can be.
    /// Show the page's pieces, changing only the ones that changed. Pieces that
    /// stay keep their place in the model, so one being dragged (a slider) keeps
    /// its pointer even when pieces before or after it come or go.
    fn show_cells(&mut self, cells: Vec<Cell>) {
        sync_cells(&self.cells, cells);
    }

    /// What the keyboard's firmware can do beyond EVGA's.
    fn firmware(&self) -> serde_json::Value {
        let v = self.page.dev.as_ref().and_then(|d| d["firmware"].as_u64()).unwrap_or(0) as u16;
        ectotype_core::firmware::describe(v)
    }

    // ------------------------------------------------------------ the sliders' jobs

    /// A dropdown on the Lighting page opened: its choices, under it, as wide as it is.
    fn open_light_menu(&mut self, c: Ctl, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        let rows: Vec<MenuRow> = if c == Ctl::Plays {
            plays_rows(&self.page)
        } else {
            let Some((list, chosen)) = lighting::choices(&self.page, c) else { return };
            list.iter().enumerate().map(|(j, label)| MenuRow { kind: 0, label: (*label).into(), tag: "".into(), tick: j == chosen, sub: "".into() }).collect()
        };
        self.close_menu();
        self.close_picker();
        let h = 6.0 + rows.iter().map(|r| if r.kind == 2 { 11.0 } else { 33.0 }).sum::<f32>() + 6.0;
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        let w = if c == Ctl::Submode { lighting::SET_W } else { lighting::PLACE_W } - 32.0;
        self.ui.set_menu_w(w);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        let (ax, ay, ah) = anchor;
        let win = self.ui.get_window_height();
        let (below, above) = (win - ay - ah - 14.0, ay - 70.0);
        let down = h <= below || below >= h.min(260.0) || below >= above;
        let room = if down { below } else { above };
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_ay(ay);
        self.ui.set_menu_ah(ah);
        self.ui.set_menu_top(if down { ay + ah + 6.0 } else { ay - 6.0 - h.min(room) });
        self.ui.set_menu_max(if h > room { room } else { win * 0.7 });
        self.ui.set_menu_open(true);
        self.light_menu = Some(c);
    }

    fn open_bind_menu(&mut self, i: usize, anchor: (f32, f32, f32)) {
        self.close_menu();
        let name = if i == 0 { "dimmer" } else { "volume" };
        let mut rows = vec![MenuRow { kind: 1, label: format!("The {name} controls").into(), tag: "".into(), tick: false, sub: "".into() }];
        for (v, label) in crate::display::BIND_CHOICES[i] {
            rows.push(MenuRow { kind: 0, label: label.into(), tag: "".into(), tick: self.bind[i] == v, sub: "".into() });
        }
        let list: f32 = rows.iter().map(|r| if r.kind == 1 { 28.0 } else { 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        self.ui.set_menu_w(240.0);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        let (ax, ay, ah) = anchor;
        let win = self.ui.get_window_height();
        // under the button, or over it when below is cramped, as every menu
        // opens, and never up onto the title bar
        let (below, above) = (win - ay - ah - 14.0, ay - 70.0);
        let down = h <= below || below >= h.min(260.0) || below >= above;
        let room = if down { below } else { above };
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_ay(ay);
        self.ui.set_menu_ah(ah);
        self.ui.set_menu_top(if down { ay + ah + 6.0 } else { ay - 6.0 - h.min(room) });
        self.ui.set_menu_max(if h > room { room } else { win * 0.7 });
        self.ui.set_menu_open(true);
        self.bind_menu = Some((i, anchor));
    }

    /// One job, one slider: taking a job the other one does leaves it idle,
    /// rather than the two of them sending it different positions.
    fn set_slider_bind(&mut self, i: usize, v: &str) {
        let other = 1 - i;
        self.bind[i] = v.into();
        if v != "none" && crate::display::drives(other, &self.bind[other]) == crate::display::drives(i, v) {
            // the other slider did this job: it rests, and remembers the job
            self.idled[other] = Some(std::mem::replace(&mut self.bind[other], "none".into()));
        } else if let Some(job) = self.idled[other].take() {
            // this slider has given up the other's job: the other has it back
            if self.bind[other] == "none" && crate::display::drives(other, &job) != crate::display::drives(i, v) {
                self.bind[other] = job;
            }
        }
        // (a slider given a job of its own forgets the one it rested from)
        self.idled[i] = None;
        crate::uistate::set("slider_bind", serde_json::json!(self.bind));
        crate::uistate::set("slider_idled", serde_json::json!(self.idled));
        self.hold_backlight();
        self.sent = [None, None];
        self.tell_modes();
    }

    /// Quitting: the keyboard does its sliders' jobs by itself again, so
    /// they keep working with Ectodeck gone.
    pub fn release_sliders(&mut self) {
        let fw = self.firmware();
        if self.kb.is_none() || self.released.is_some() {
            return;
        }
        if crate::display::has(&fw, "volume.quiet") || crate::display::has(&fw, "dimmer.keep") {
            self.ask_kb(device::Ask::Modes(false, false));
        }
        if crate::display::has(&fw, "dimmer.set") {
            self.ask_kb(device::Ask::Dimmer(None));
        }
        self.released = Some(Instant::now());
    }

    /// The backlight follows the dimmer inside the keyboard, so taking the
    /// dimmer off it means asking the keyboard to hold the backlight where it is.
    fn hold_backlight(&mut self) {
        if self.bind[0] == "own" {
            self.held = None;
            self.ask_kb(device::Ask::Dimmer(None));
            return;
        }
        if self.held.is_none() {
            self.held = Some(self.last_sliders.as_ref().and_then(|r| r["dimmer"].as_u64()).unwrap_or(255) as u8);
        }
        self.ask_kb(device::Ask::Dimmer(self.held));
    }

    /// From firmware build 3 the keyboard can be asked to stop doing a
    /// slider's job itself, so the two do not fight.
    fn tell_modes(&mut self) {
        let fw = self.firmware();
        let (vq, dk) = (crate::display::has(&fw, "volume.quiet"), crate::display::has(&fw, "dimmer.keep"));
        if vq || dk {
            self.ask_kb(device::Ask::Modes(vq && self.bind[1] != "own", dk && self.bind[0] != "own"));
        }
    }

    /// Set what a slider drives, but only once it has really moved, so opening
    /// Ectodeck never throws the volume to wherever the slider sits.
    fn drive_sliders(&mut self, r: &serde_json::Value, f: [f32; 2]) {
        // the dimmer's level, which every key's light inside the keyboard is scaled by
        self.page.dimmer = r["dimmer"].as_u64().map(|v| v.min(255) as u8);
        let prev = self.last_sliders.replace(r.clone());
        let Some(prev) = prev else { return };
        let raw = |v: &serde_json::Value, k: &str| v[k].as_f64().unwrap_or(0.0);
        let moved = [(raw(r, "dimmer_raw") - raw(&prev, "dimmer_raw")).abs() > 12.0, (raw(r, "volume_raw") - raw(&prev, "volume_raw")).abs() > 12.0];
        let mut send = |me: &mut Self, i: usize, target: &str| {
            let level = (f[i] as f64 * 100.0).round() / 100.0;
            if me.sent[i] == Some(level) {
                return;
            }
            me.sent[i] = Some(level);
            if target == "backlight" {
                me.held = Some((level * 255.0).round() as u8);
                me.ask_kb(device::Ask::Dimmer(me.held));
            } else {
                set_audio(target, level);
            }
        };
        if self.bind[0] != "own" {
            // the keyboard lets a held level go when its slider moves; hold it again
            if !r["dimmer_held"].as_bool().unwrap_or(false) && self.held.is_some() {
                self.ask_kb(device::Ask::Dimmer(self.held));
            }
            if moved[0] && (self.bind[0] == "output" || self.bind[0] == "microphone") {
                let t = self.bind[0].clone();
                send(self, 0, &t);
            }
        }
        if moved[1] && (self.bind[1] == "backlight" || self.bind[1] == "microphone") {
            let t = self.bind[1].clone();
            send(self, 1, &t);
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

    // ------------------------------------------------------------ the screen

    /// Ectodeck playing the screen, from now on.
    pub fn set_player(&mut self, p: crate::lcd_player::Player) {
        if self.lcd_paused {
            p.tell(crate::lcd_player::Control::Paused(true));
        }
        self.player = Some(p);
    }

    /// The player said what the screen shows now.
    pub fn screen_news(&mut self, n: crate::lcd_player::News) {
        let state_changed = n.state != self.lcd.state || n.sequence != self.lcd.sequence;
        self.lcd = n;
        if self.page.view == View::Screen && self.ui.get_place() == "z10" {
            if state_changed {
                self.refresh();
            } else {
                self.draw_screen();
            }
        }
    }

    /// Ectodeck closing: the screen handed back to the keyboard's own screens.
    pub fn quit_screen(&mut self) {
        if let Some(p) = self.player.take() {
            p.quit(true);
        }
    }

    /// What the screen is doing now, said beside Pause on the Screen tab.
    fn screen_status(&self) -> &'static str {
        use crate::lcd_player::State;
        match (self.lcd.state, self.lcd.sequence) {
            (_, _) if !self.controlled => "Ectodeck leaves this keyboard to other programs.",
            (State::Other, _) => "Another program is using the screen. Ectodeck plays it once that stops.",
            (State::Missing, _) => "Ectodeck can’t reach the keyboard’s screen.",
            (_, Some("launch")) => "A game is starting",
            (_, Some(_)) => "A game just closed",
            (State::Paused, _) => "Paused on Stats",
            (State::Playing, None) => "Now playing Stats",
        }
    }

    /// The screen drawn large, the playlist's picture of Stats, and its
    /// picture of a launch, each in the profile's screen light, when changed.
    fn draw_screen(&mut self) {
        use crate::lcd_player::State;
        let state = match self.lcd.state {
            _ if !self.controlled => 4,
            State::Playing => 0,
            State::Paused => 1,
            State::Other => 2,
            State::Missing => 3,
        };
        self.ui.set_lcd_state(state);
        if self.page.dev.is_none() {
            return;
        }
        let (light, _) = screen_of(&self.page.draft()["lighting"]);
        let (level, rgb) = (light[0], [light[1], light[2], light[3]]);
        let sc = self.scale.max(1.0);
        let px = |w: f32| (w * sc).round() as u32;
        let key = (self.lcd.rows.clone(), rgb, level, px(860.0));
        if self.lcd_drawn.as_ref() != Some(&key) {
            let lv = level as f32 / 255.0;
            self.ui.set_lcd_big(crate::lcd_draw::picture(&self.lcd.rows, rgb, lv, px(860.0), px(172.0)));
            self.ui.set_lcd_thumb(crate::lcd_draw::picture(&self.lcd.rows, rgb, lv, px(150.0), px(30.0)));
            self.lcd_drawn = Some(key);
        }
        let i = self.launch_at.0;
        let key = (i, rgb, level, px(150.0));
        if self.launch_drawn != Some(key) {
            if let Some((rows, _)) = self.launch_frames.get(i) {
                self.ui.set_lcd_launch(crate::lcd_draw::picture(rows, rgb, level as f32 / 255.0, px(150.0), px(30.0)));
            }
            self.launch_drawn = Some(key);
        }
    }

}

/// The launch sequence the playlist plays as its picture: the frames Grant
/// approved for game-helper's launches (DEEPDRIVER II), each with its time.
fn launch_frames() -> Vec<(Vec<String>, u32)> {
    let v: serde_json::Value = serde_json::from_str(include_str!("../data/lcd-launch.json")).unwrap_or_default();
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|f| (f["r"].as_array().map(|r| r.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default(), f["ms"].as_u64().unwrap_or(40) as u32))
                .collect()
        })
        .unwrap_or_default()
}

/// The colours used lately, newest first, as the picker lists them.
fn recent_colors() -> Vec<[u8; 3]> {
    crate::uistate::get("recent_colours").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn keep_recent(c: [u8; 3]) {
    let mut list = vec![c];
    list.extend(recent_colors().into_iter().filter(|r| *r != c));
    list.truncate(9);
    crate::uistate::set("recent_colours", serde_json::json!(list));
}

/// Set the computer's output or microphone volume, 0 to 1, exactly where the
/// slider is, instead of sending volume keys.
fn set_audio(target: &str, level: f64) {
    let node = match target {
        "output" => "@DEFAULT_AUDIO_SINK@",
        "microphone" => "@DEFAULT_AUDIO_SOURCE@",
        _ => return,
    };
    let level = format!("{:.3}", level.clamp(0.0, 1.0));
    std::thread::spawn(move || {
        let _ = std::process::Command::new("wpctl").args(["set-volume", node, &level]).status();
    });
}

/// The place Settings names for the weather, as saved.
fn weather_place(settings: &crate::settings::Settings) -> Option<ectodeck_weather::At> {
    let p = settings.get("weather.place")?;
    Some(ectodeck_weather::At { lat: p["lat"].as_f64()? as f32, lon: p["lon"].as_f64()? as f32, country: p["country_code"].as_str().unwrap_or("").to_string() })
}

impl Front {
    /// How much brighter than the page's colours the keys' light is drawn:
    /// the design's brightness, unless Settings › HDR lets it go to a peak.
    pub fn light_gain(&self, reference: f64) -> f32 {
        let hdr = self.settings.get("hdr").unwrap_or_default();
        if hdr["lighting"].as_str() == Some("off") {
            return 1.0;
        }
        // up to Peak nits; at the slider's left end, the design's brightness
        hdr["glow_limit"].as_f64().map_or(1.0, |n| (n / reference).max(1.0) as f32)
    }
}

/// Bring a model to `rows` in place: each row that changed is changed, rows
/// are added or taken off the end, and the rest are left alone, so an item
/// being clicked is never rebuilt under the pointer.
pub fn set_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    for (i, r) in rows.iter().enumerate() {
        if i < model.row_count() {
            if model.row_data(i).as_ref() != Some(r) {
                model.set_row_data(i, r.clone());
            }
        } else {
            model.push(r.clone());
        }
    }
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
}

/// A profile's screen: its light (brightness, red, green, blue) and its built-in screens.
fn screen_of(l: &serde_json::Value) -> ([u8; 4], u8) {
    let c = &l["lcd"]["color"];
    let b = |v: &serde_json::Value| v.as_u64().unwrap_or(0).min(255) as u8;
    ([c["brightness"].as_u64().map_or(255, |v| v.min(255) as u8), b(&c["color"][0]), b(&c["color"][1]), b(&c["color"][2])], b(&l["lcd"]["apps"]))
}

/// Show a list in one of the page's models, in place (see [set_rows]): a list
/// set anew rebuilds every item, and the page is refreshed after every
/// reading from the keyboard (ten a second, while the sliders are read), so a
/// click on a list set anew each time was often lost between its press and
/// its release.
fn show_rows<T: Clone + PartialEq + 'static>(current: ModelRc<T>, set: impl FnOnce(ModelRc<T>), rows: Vec<T>) {
    match current.as_any().downcast_ref::<VecModel<T>>() {
        Some(m) => set_rows(m, rows),
        None => set(ModelRc::new(VecModel::from(rows))),
    }
}

/// Bring a model of pieces to `rows`: the same pieces at the start and at the
/// end are kept (their data updated in place), and only those between are
/// taken out and put in. A piece is the same piece by its kind, id and index.
pub fn sync_cells(model: &VecModel<Cell>, rows: Vec<Cell>) {
    let key = |c: &Cell| (c.kind, c.id, c.index);
    let old: Vec<Cell> = (0..model.row_count()).filter_map(|i| model.row_data(i)).collect();
    let (n, m) = (old.len(), rows.len());
    let mut head = 0;
    while head < n && head < m && key(&old[head]) == key(&rows[head]) {
        head += 1;
    }
    let mut tail = 0;
    while tail < n - head && tail < m - head && key(&old[n - 1 - tail]) == key(&rows[m - 1 - tail]) {
        tail += 1;
    }
    for i in (head..n - tail).rev() {
        model.remove(i);
    }
    for (k, c) in rows[head..m - tail].iter().enumerate() {
        model.insert(head + k, c.clone());
    }
    for i in (0..head).chain(m - tail..m) {
        if model.row_data(i).as_ref() != Some(&rows[i]) {
            model.set_row_data(i, rows[i].clone());
        }
    }
}
