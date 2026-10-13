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

/// What a row of the Sensors card's menus chooses: a CPU sensor by its driver and label, or
/// a graphics card by its PCI address and name.
#[derive(Clone, Debug, PartialEq)]
pub enum SensorPick {
    Cpu(String, String),
    Card(String, String),
}

/// What a row of the New screen layout menu makes: a blank layout, or a copy of the
/// layout with this id.
#[derive(Clone, Debug, PartialEq)]
pub enum NewFrom {
    Scratch,
    Copy(String),
}

/// The New screen layout menu, in either window: From scratch, then under a heading every
/// layout there is, to start from a copy of it; and what each row makes (None for the
/// rule and the heading).
pub fn new_rows(library: &[crate::lcd_page::Page]) -> (Vec<MenuRow>, Vec<Option<NewFrom>>) {
    let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into() };
    let mut rows = vec![row(0, "From scratch"), row(2, ""), row(1, "Duplicate a layout")];
    let mut made = vec![Some(NewFrom::Scratch), None, None];
    for p in library {
        rows.push(row(0, &p.name));
        made.push(Some(NewFrom::Copy(p.id.clone())));
    }
    (rows, made)
}

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
    /// the Screen Editor, on the layout the screen shows
    ScreenEditor,
    /// a layout card's menu, right-clicked: the layout, and where the pointer was
    LibMenu(i32, f32, f32),
    /// the question before a layout is deleted, answered Delete or Cancel
    ConfirmYes,
    ConfirmNo,
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
    /// the Screen tab: Pause or Play; time for the readings to be read again
    LcdPause,
    LcdTick,
    /// the playlist's strip: a screen picked up from the strip (0, a turn) or
    /// the Library (1, a page); carried to a place over the strip (or away
    /// from it, false); let go, settled into its room; put back
    CarryStart(i32, i32),
    CarryMove(f32, bool),
    CarryDrop,
    CarryCancel,
    /// a turn's right edge dragged so far, and let go
    SizeTurn(i32, f32),
    SizeDone,
    /// a turn's way bar selected, its menu of ways in to open under it
    WayMenu(i32, f32, f32, f32),
    /// a turn's time selected, its menu of times to open under it (the time's place and size)
    PlTime(i32, f32, f32, f32, f32),
    /// the panel's playlist selected, its menu to open under it; its name
    /// changed in place, kept (true) or let go, as typed
    PlChoose(f32, f32, f32),
    PlRenameDone(bool, String),
    /// the playlist's holding bar selected: the layouts to add, in a menu under it
    PlHoldMenu(f32, f32, f32),
    /// a script's screen ended, the playlist back on the screen
    PlBack,
    /// the Screen tab's other state opened: while Ectodeck runs, or when it is closed
    ScreenState,
    /// the onboard layout's chooser selected, where it is in the window
    ObChoose(f32, f32, f32),
    /// a turn's layout, from the playlist, opened in the screen layout editor
    EditPage(i32),
    /// the playlist: a page shown now, moved earlier or later, taken off; a
    /// new screen layout made (blank, and opened in the editor)
    PageShow(i32),
    /// a page moved from one place in the playlist to another
    PageMove(i32, i32),
    PageRemove(i32),
    /// "New screen layout" selected, where the pointer was
    LibNew(f32, f32),
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

pub(crate) const PRESETS: [[u8; 3]; 9] = [[255, 255, 255], [255, 0, 0], [255, 96, 0], [255, 200, 0], [0, 255, 0], [0, 255, 200], [0, 120, 255], [120, 0, 255], [255, 0, 180]];

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
    /// open the screen layout editor's window, on this layout of the Library
    OpenLayouts(usize),
    /// bring the main window forward on Ectodeck's Settings (from the screen editor's
    /// From line, which links to where its sensor is chosen)
    ShowSettings,
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
    /// the window closed into the tray, out of sight
    hidden: bool,
    /// the player told to stand aside, the keyboard showing its own status screen for the
    /// Screen tab's picture of it (see [Front::status_showing])
    aside: bool,
    /// the onboard layout's chooser open; on the second tab, a layout carried over the
    /// screen, by its id, which the picture shows before it is let go
    ob_menu: bool,
    ob_over: Option<String>,
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
    /// the draft changed while a preview was asked for: sent once it is answered
    preview_again: bool,
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
    /// when the page on the screen came on, for its time left
    page_from: Instant,
    lcd_drawn: Option<(Vec<String>, [u8; 3], u8, u32)>,
    /// each playlist page's picture, and what it was drawn from
    pl_thumbs: Vec<Option<(PicKey, slint::Image)>>,
    /// the sheet of pages to add: open, and the page selected on it
    /// window asks made while a menu's choice was carried out
    pending_window: Vec<WindowAsk>,
    /// the screen layout editor's window, made the first time it opens
    pub layouts: Option<crate::layout_page::LayoutPage>,
    /// the screen's Library and playlist (the same on every profile), as
    /// last noted for undo and as last saved, and when they last changed
    screens: crate::lcd_page::Screens,
    screens_noted: crate::lcd_page::Screens,
    screens_saved: crate::lcd_page::Screens,
    pages_changed: Option<Instant>,
    /// editing: the playlist's turn that showed before, which shows again after
    edit_from: usize,
    /// the screen layout editor's window is open (with a layout open in it, or
    /// none, once every layout is deleted)
    layouts_shown: bool,
    /// the layout open in the editor, and when it came up: its scrolling words are timed
    /// from then, in the editor's picture and the Screen tab's
    edit_clock: Option<(String, Instant)>,
    /// the profile whose playlist the player was last given: the one the page shows
    played_by: u8,
    /// the strip: each turn, a screen being carried to it, a turn's end
    /// being dragged (the turn, its time when taken, the strip's scale then),
    /// the menu of ways in open on a way bar, the menu of times open on a
    /// turn's time; the strip's scale, its way bars' length and the turns
    /// shown (without a carried one's room), as last laid out
    blocks: Rc<VecModel<crate::PlBlock>>,
    carry: Option<Carry>,
    sizing: Option<(usize, u32, f64)>,
    way_menu: Option<usize>,
    time_menu: Option<TimeMenu>,
    strip_pps: f64,
    strip_way: f64,
    strip_spans: Vec<(f64, f64)>,
    /// the panel's menu of playlists open; the playlist's name being changed in place; the
    /// holding bar's menu of layouts to add open
    pl_menu: bool,
    pl_renaming: bool,
    add_menu: bool,
    /// "New screen layout"'s menu: what each row makes
    new_menu: Option<Vec<Option<NewFrom>>>,
    /// the Sensors card's menus: what each row chooses (a CPU sensor by its driver and
    /// label, a card by its PCI address and name); and what the card shows, read once a second
    sensor_menu: Option<Vec<Option<SensorPick>>>,
    sensors_seen: Option<(Instant, crate::ecto_settings::Sensors)>,
    /// where the readings with a choice come from, for the editor, as last looked at
    from_seen: Option<(Instant, crate::sensors::Choice, crate::screen_edit::From)>,
    /// a layout card's own menu, the card selected or right-clicked: the layout's place in
    /// the Library; and the card whose menu a press just closed, so its click closes it
    lib_menu: Option<usize>,
    card_shut: Option<(usize, Instant)>,
    /// the layout asked about before it is deleted, by its id
    deleting: Option<String>,
    /// the page being edited, the piece chosen; this computer's readings,
    /// read here for the editor's picture once a second, and when last
    editor: Option<crate::screen_edit::Editor>,
    reader: Option<crate::lcd_stats::Stats>,
    readings: crate::lcd_page::Readings,
    read_at: Option<Instant>,
}

/// A profile's colour sample: how many colours, and the colours.
fn swatch(cols: &[[f64; 3]]) -> (i32, [Color; 4]) {
    let c = |i: usize| cols.get(i).or(cols.last()).map_or(Color::from_rgb_u8(0, 0, 0), |v| Color::from_rgb_u8(v[0] as u8, v[1] as u8, v[2] as u8));
    (cols.len().clamp(1, 4) as i32, [c(0), c(1), c(2), c(3)])
}

impl Front {
    pub fn new(ui: AppWindow, kb: Option<mpsc::Sender<device::Ask>>, controlled: bool, settings: crate::settings::Settings) -> Front {
        // (where the screen's readings come from, as last chosen)
        crate::sensors::choose(crate::sensors::Choice::of(&settings.all()["sensors"]));
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
        on!(on_screen_editor, Ask::ScreenEditor);
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
        on!(on_screen_edit, |i| Ask::EditPage(i));
        on!(on_pl_carry_start, |from, i| Ask::CarryStart(from, i));
        on!(on_pl_carry_move, |x, near| Ask::CarryMove(x, near));
        on!(on_pl_carry_drop, Ask::CarryDrop);
        on!(on_pl_carry_cancel, Ask::CarryCancel);
        on!(on_pl_size, |i, dx| Ask::SizeTurn(i, dx));
        on!(on_pl_size_done, Ask::SizeDone);
        on!(on_pl_mark, |i, x, y, h| Ask::WayMenu(i, x, y, h));
        on!(on_pl_time, |i, x, y, w, h| Ask::PlTime(i, x, y, w, h));
        on!(on_pl_choose, |x, y, h| Ask::PlChoose(x, y, h));
        on!(on_pl_rename_done, |keep, t| Ask::PlRenameDone(keep, t.to_string()));
        on!(on_pl_hold_menu, |x, y, h| Ask::PlHoldMenu(x, y, h));
        on!(on_pl_back, Ask::PlBack);
        on!(on_screen_state, Ask::ScreenState);
        on!(on_ob_choose, |x, y, h| Ask::ObChoose(x, y, h));
        on!(on_lib_menu, |k, x, y| Ask::LibMenu(k, x, y));
        on!(on_confirm_yes, Ask::ConfirmYes);
        on!(on_confirm_no, Ask::ConfirmNo);
        on!(on_page_show, |i| Ask::PageShow(i));
        on!(on_page_moved, |i, j| Ask::PageMove(i, j));
        on!(on_page_remove, |i| Ask::PageRemove(i));
        on!(on_lib_new, |x, y| Ask::LibNew(x, y));
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
            hidden: false,
            aside: false,
            ob_menu: false,
            ob_over: None,
            overlays: Overlays::new(),
            sliders_asked: false,
            active_asked: false,
            slider_ends,
            _timers: timers,
            paint: lighting::PRESETS[0],
            previewing: false,
            preview_asked: false,
            preview_again: false,
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
            lcd: crate::lcd_player::News { rows: Vec::new(), sequence: None, state: crate::lcd_player::State::Missing, page: 0, turning: false, elapsed: 0.0, length: 0.0 },
            lcd_paused: false,
            page_from: Instant::now(),
            lcd_drawn: None,
            pl_thumbs: Vec::new(),
            pending_window: Vec::new(),
            layouts: None,
            screens: crate::lcd_page::Screens::default(),
            screens_noted: crate::lcd_page::Screens::default(),
            screens_saved: crate::lcd_page::Screens::default(),
            pages_changed: None,
            edit_from: 0,
            layouts_shown: false,
            edit_clock: None,
            played_by: 0,
            blocks: Rc::new(VecModel::default()),
            carry: None,
            sizing: None,
            way_menu: None,
            time_menu: None,
            strip_pps: 0.0,
            strip_way: WAY_W.1,
            strip_spans: Vec::new(),
            pl_menu: false,
            pl_renaming: false,
            add_menu: false,
            new_menu: None,
            sensor_menu: None,
            sensors_seen: None,
            from_seen: None,
            lib_menu: None,
            card_shut: None,
            deleting: None,
            editor: None,
            reader: None,
            readings: crate::lcd_page::Readings::default(),
            read_at: None,
        };
        f.screens = crate::lcd_page::Screens::of(
            f.settings.get("devices.z10.screen.pages").as_ref(),
            f.settings.get("devices.z10.screen.playlists").as_ref(),
            f.settings.get("devices.z10.screen.plays").as_ref(),
            f.settings.get("devices.z10.screen.playlist").as_ref(),
        )
        .with_onboard(f.settings.get("devices.z10.screen.onboard").as_ref());
        f.screens_noted = f.screens.clone();
        f.screens_saved = f.screens.clone();
        f.weather.set_place(weather_place(&f.settings));
        f.ui.set_overlays(ModelRc::from(f.overlay_model.clone()));
        f.ui.set_pk_presets(ModelRc::new(VecModel::from(PRESETS.iter().map(|c| color(*c)).collect::<Vec<_>>())));
        f.ui.set_cells(ModelRc::from(f.cells.clone()));
        f.ui.set_place_cells(ModelRc::from(f.place_cells.clone()));
        f.ui.set_pl_blocks(ModelRc::from(f.blocks.clone()));
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
        // a save or a switch of profile has the keyboard apply a stored profile again,
        // its screen's light and screens too: what Ectodeck last sent is no longer there
        if matches!(h, device::Heard::Saved(..) | device::Heard::KeysSaved(..) | device::Heard::Active(..)) {
            self.screen_sent = None;
        }
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
                // what changed while this one was on its way, now (still previewing)
                if std::mem::take(&mut self.preview_again) {
                    self.push_preview();
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
                        self.status(format!("The keyboard switched to {}.", self.page.profile_name(n)), 0);
                    }
                }
            }
            device::Heard::Active(n, r) => match r {
                Ok(active) => {
                    self.page.set_active(active.unwrap_or(n));
                    self.status(format!("{} is now active on the keyboard.", self.page.profile_name(n)), 1);
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
                Ask::EditPage(i) => {
                    // a turn's layout, from the playlist
                    if let Some(k) = self.screens.turns(self.page.editing).get(i.max(0) as usize).and_then(|e| self.screens.index_of(&e.page)) {
                        window.push(WindowAsk::OpenLayouts(k));
                    }
                }
                Ask::PageShow(i) => {
                    if let Some(pl) = &self.player {
                        pl.tell(crate::lcd_player::Control::Show(i.max(0) as usize));
                    }
                }
                Ask::PageMove(i, j) => {
                    let (i, j) = (i.max(0) as usize, j.max(0) as usize);
                    let shown = self.lcd.page;
                    let playing = self.screens.turns(self.page.editing).get(shown).map(|e| e.key);
                    if i != j && self.screens.move_turn(self.page.editing, i, j).is_some() {
                        self.pages_edited();
                        self.keep_playing(playing, shown);
                    }
                }
                Ask::PageRemove(i) => {
                    let i = i.max(0) as usize;
                    let shown = self.lcd.page;
                    let playing = self.screens.turns(self.page.editing).get(shown).map(|e| e.key);
                    // (the last one too: the screen shows blank until a layout is added)
                    if self.screens.remove_turn(self.page.editing, i) {
                        self.pages_edited();
                        self.keep_playing(playing, shown);
                    }
                }
                // a new screen layout: a blank one, straight into the editor
                // (with no layout to copy, a blank one at once)
                Ask::LibNew(x, y) if !self.screens.library.is_empty() => self.open_new_menu((x, y)),
                Ask::LibNew(..) => {
                    self.close_menu();
                    let i = self.screens.new_page();
                    self.pages_edited();
                    window.push(WindowAsk::OpenLayouts(i));
                }
                Ask::LcdPause => {
                    self.lcd_paused = !self.lcd_paused;
                    if let Some(pl) = &self.player {
                        pl.tell(crate::lcd_player::Control::Paused(self.lcd_paused));
                    }
                }
                Ask::CarryStart(from, i) => {
                    let i = i.max(0) as usize;
                    self.close_menu();
                    self.carry = if from == 0 {
                        self.screens.turns(self.page.editing).get(i).map(|e| Carry { from_strip: true, index: i, page: e.page.clone(), secs: e.secs, slot: Some(i), home: Some(i) })
                    } else {
                        self.screens.library.get(i).map(|p| Carry { from_strip: false, index: i, page: p.id.clone(), secs: crate::lcd_page::SECS.2, slot: None, home: None })
                    };
                    if let Some(p) = self.carry.as_ref().and_then(|c| self.screens.library.iter().find(|p| p.id == c.page)) {
                        self.ui.set_carry_name(p.name.as_str().into());
                    }
                }
                Ask::CarryMove(x, near) => {
                    changed = false;
                    let spans = self.strip_spans.clone();
                    // on the second tab the screen is where a layout lands: over the screen, the
                    // picture shows the layout as the keyboard would show it
                    if self.ui.get_screen_closed() {
                        let over = if near { self.carry.as_ref().map(|c| c.page.clone()) } else { None };
                        if over != self.ob_over {
                            self.ob_over = over;
                            self.lcd_drawn = None;
                            changed = true;
                        }
                    } else if let Some(c) = &mut self.carry {
                        let slot = if near { Some(slot_at(&spans, x as f64)) } else { c.home };
                        if slot != c.slot {
                            c.slot = slot;
                            changed = true;
                        }
                    }
                }
                Ask::CarryDrop => {
                    if let Some(c) = self.carry.take() {
                        if let Some(id) = self.ob_over.take() {
                            self.screens.keep_onboard(self.page.editing, Some(&id));
                            self.pages_edited();
                        } else if let Some(at) = c.slot {
                            let shown = self.lcd.page;
                            let playing = self.screens.turns(self.page.editing).get(shown).map(|e| e.key);
                            let moved = if c.from_strip {
                                at != c.index && self.screens.move_turn(self.page.editing, c.index, at).is_some()
                            } else {
                                self.screens.add_turn(self.page.editing, at, &c.page);
                                true
                            };
                            if moved {
                                self.pages_edited();
                                self.keep_playing(playing, shown);
                            }
                        }
                    }
                }
                Ask::CarryCancel => {
                    self.carry = None;
                    self.ob_over = None;
                }
                Ask::SizeTurn(i, dx) => {
                    let i = i.max(0) as usize;
                    if self.sizing.is_none_or(|s| s.0 != i) {
                        self.close_menu();
                        self.sizing = self.screens.turns(self.page.editing).get(i).map(|e| (i, e.secs, self.strip_pps));
                    }
                    // (a turn that stayed until the layouts were switched is timed from the drag on)
                    if let Some((i, from, pps)) = self.sizing {
                        let secs = (from as f64 + dx as f64 / pps.max(1e-6)).round().clamp(crate::lcd_page::SECS.0 as f64, crate::lcd_page::SECS.1 as f64) as u32;
                        if let Some(e) = self.screens.turns_mut(self.page.editing).get_mut(i) {
                            if e.secs != secs || e.stays {
                                (e.secs, e.stays) = (secs, false);
                                self.pages_edited();
                            }
                        }
                    }
                    self.ui.set_pl_live(true);
                }
                Ask::SizeDone => {
                    self.sizing = None;
                    self.ui.set_pl_live(false);
                }
                Ask::WayMenu(i, x, y, h) => self.open_way_menu(i.max(0) as usize, (x, y, h)),
                Ask::PlTime(i, x, y, w, h) => self.open_time_menu(i.max(0) as usize, (x, y, h), w),
                Ask::PlChoose(x, y, h) => self.open_pl_menu((x, y, h)),
                Ask::PlHoldMenu(x, y, h) => self.open_add_menu((x, y, h)),
                Ask::PlRenameDone(keep, t) => self.end_rename(keep, &t),
                Ask::PlBack => {
                    if let Some(pl) = &self.player {
                        pl.tell(crate::lcd_player::Control::EndSequence);
                    }
                }
                // (the screen drawn again, as the state now open shows it)
                Ask::ScreenState => self.lcd_drawn = None,
                Ask::ObChoose(x, y, h) => self.open_ob_menu((x, y, h)),
                Ask::LibMenu(k, x, y) => self.open_lib_menu(k.max(0) as usize, (x, y)),
                Ask::ConfirmYes => {
                    if let Some(k) = self.deleting.take().and_then(|id| self.screens.index_of(&id)) {
                        self.delete_layout(k);
                    }
                    self.ui.set_confirm_open(false);
                }
                Ask::ConfirmNo => {
                    self.deleting = None;
                    self.ui.set_confirm_open(false);
                }
                Ask::LcdTick => {
                    // the readings again each second: for the page being edited (its
                    // picture and its boxes), or the playlist's pictures of its pages
                    let on_screen = self.page.view == View::Screen && self.ui.get_place() == "z10";
                    if (self.editor.is_some() || on_screen) && self.read_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
                        self.read_now();
                        if self.editor.is_some() {
                            self.refresh_editor();
                        }
                        self.draw_screen();
                    } else if self.edit_moving() {
                        // words scrolling in the layout being edited: its pictures move on
                        // between readings (drawn again only when a row has changed)
                        self.refresh_editor();
                        self.draw_screen();
                    }
                    // the Sensors card's readings, once a second while Settings is open
                    if self.ui.get_place() == "settings" && self.sensors_seen.as_ref().is_some_and(|(t, _)| t.elapsed() >= Duration::from_secs(1)) {
                        self.refresh();
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
                    // (the keyboard picture shows the layer open)
                    self.page.fn_layer = self.layer == 1;
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
                // (not while the window asks before deleting a layout)
                Ask::Undo(redo) if self.deleting.is_none() => self.undo(redo),
                Ask::Undo(_) => {}
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
                    // the profile the keyboard is using and the weather, for the pages that show them
                    self.tell_context();
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
                Ask::ScreenEditor => {
                    self.close_menu();
                    // (on the layout the keyboard's screen shows now, or else the first)
                    let i = self.shown().and_then(|p| self.screens.index_of(&p.id)).unwrap_or(0);
                    window.push(WindowAsk::OpenLayouts(i));
                }
            }
            any |= changed;
        }
        if any {
            self.refresh();
        }
        // (asked for away from an ask: the onboard chooser's Edit)
        window.extend(self.pending_window.drain(..));
        window
    }

    /// The keyboard's light as it lights it now: which caps are pale.
    pub fn light_caps(&mut self, light: &serde_json::Value) {
        crate::z10::set_bleeds(light);
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
        if !self.previewing || self.page.dev.is_none() || self.kb.is_none() {
            return;
        }
        // one asked for at a time; a change made meanwhile goes once it is answered
        if self.preview_asked {
            self.preview_again = true;
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
            Ctl::CpuSensor | Ctl::GpuCard => {
                self.open_sensor_menu(c, _anchor);
                return;
            }
            Ctl::CpuClock => {
                let mut ch = crate::sensors::choice();
                ch.fastest = index == 1;
                self.keep_sensors(ch);
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
        // the screen's pages, undone as one entry (0, which no profile is)
        if self.screens != self.screens_noted {
            let burst = self.undo_mark.is_some_and(|(m, t)| m == 0 && t.elapsed() < Duration::from_millis(1200));
            if !burst {
                self.undo.push((0, self.screens_noted.to_json()));
                if self.undo.len() > 100 {
                    self.undo.remove(0);
                }
            }
            self.undo_mark = Some((0, Instant::now()));
            self.redo.clear();
            self.screens_noted = self.screens.clone();
        }
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
        if self.pages_changed.is_some_and(|t| t.elapsed() >= Duration::from_millis(1200)) {
            self.save_pages();
        }
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
        if n == 0 {
            to.push((0, self.screens.to_json()));
            self.screens = crate::lcd_page::Screens::from_json(&d);
            self.screens_noted = self.screens.clone();
            self.undo_mark = None;
            if let Some(ed) = &mut self.editor {
                ed.page = ed.page.min(self.screens.library.len().saturating_sub(1));
                ed.chosen = ed.chosen.filter(|i| self.screens.library.get(ed.page).is_some_and(|p| *i < p.pieces.len()));
            }
            self.pages_edited();
            self.status(if redo { "Did again the last change on the Screen tab" } else { "Undid the last change on the Screen tab" }, 0);
            self.refresh();
            return;
        }
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
        self.save_pages();
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
        if self.bind_menu.take().is_some() || self.light_menu.take().is_some() || self.way_menu.take().is_some() || self.time_menu.take().is_some() || std::mem::take(&mut self.pl_menu) || std::mem::take(&mut self.add_menu) || std::mem::take(&mut self.ob_menu) || self.lib_menu.take().is_some() || self.new_menu.take().is_some() || self.sensor_menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_menu_w(300.0);
            self.ui.set_pl_menu(-1);
            self.ui.set_pl_timing(-1);
            self.ui.set_pl_choosing(false);
            self.ui.set_pl_adding(false);
            self.ui.set_ob_choosing(false);
        }
        if self.menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.invoke_blur_menu();
        }
        // (the next menu keeps a column for ticks, unless it has none)
        self.ui.set_menu_marks(true);
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
        // a press away from the playlist's name being changed keeps the name
        if self.pl_renaming {
            let (rx, ry) = (self.ui.get_pl_rename_wx(), self.ui.get_pl_rename_wy());
            if !(x >= rx && x < rx + 195.0 && y >= ry && y < ry + 28.0) {
                let typed = self.ui.get_pl_rename_text().to_string();
                self.end_rename(true, &typed);
            }
        }
        if self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.time_menu.is_some() || self.pl_menu || self.add_menu || self.ob_menu || self.lib_menu.is_some() || self.new_menu.is_some() || self.sensor_menu.is_some() {
            let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
            if !(x >= mx && x < mx + mw && y >= my && y < my + mh) {
                // (a card's menu shut by a press: if the press is on the same card, its click
                // closes the menu rather than opening it again; New screen layout's place is
                // after the last card's)
                if let Some(k) = self.lib_menu.or(self.new_menu.as_ref().map(|_| self.screens.library.len())) {
                    self.card_shut = Some((k, Instant::now()));
                }
                // a press on the dropdown (or the bar) the menu hangs from shuts it, and its click must not open it again
                let (ax, ay, ah) = (self.ui.get_menu_ax(), self.ui.get_menu_ay(), self.ui.get_menu_ah());
                let aw = if self.way_menu.is_some() {
                    self.strip_way as f32
                } else if let Some(t) = &self.time_menu {
                    t.aw
                } else if self.pl_menu {
                    self.ui.get_pl_chooser_w()
                } else if self.add_menu {
                    self.ui.get_pl_hold_w()
                } else if self.ob_menu {
                    self.ui.get_ob_chooser_w()
                } else {
                    self.ui.get_menu_w()
                };
                if (self.light_menu.is_some() || self.way_menu.is_some() || self.time_menu.is_some() || self.pl_menu || self.add_menu || self.ob_menu || self.sensor_menu.is_some()) && x >= ax && x < ax + aw && y >= ay && y < ay + ah {
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
        if self.menu.is_some() || self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.pick.is_some() || self.place_menu {
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
        // the onboard layout's chooser: the screen off, a layout for the keyboard to keep, or
        // the one it keeps opened in the editor
        if self.ob_menu {
            self.close_menu();
            let n = self.page.editing;
            let count = self.screens.library.len();
            match i {
                1 => {
                    self.screens.keep_onboard(n, None);
                    self.pages_edited();
                }
                k if (2..2 + count).contains(&k) => {
                    let id = self.screens.library[k - 2].id.clone();
                    self.screens.keep_onboard(n, Some(&id));
                    self.pages_edited();
                }
                k if k == 3 + count => {
                    if let Some(at) = self.screens.onboard_of(n).and_then(|p| self.screens.index_of(&p.id)) {
                        self.pending_window.push(WindowAsk::OpenLayouts(at));
                    }
                }
                _ => {}
            }
            self.refresh();
            return;
        }
        // the panel's menu: another playlist for the profile, a new one, this one
        // renamed in place or deleted
        if self.pl_menu {
            self.close_menu();
            let n = self.page.editing;
            let count = self.screens.playlists.len();
            let mine = self.screens.of_profile(n);
            match i {
                k if (1..=count).contains(&k) => {
                    if k - 1 != mine {
                        let id = self.screens.playlists[k - 1].id.clone();
                        self.screens.choose(n, &id);
                        self.pages_edited();
                        self.play_from_start();
                    }
                }
                k if k == count + 2 => {
                    self.screens.new_playlist(n);
                    self.pages_edited();
                    self.play_from_start();
                }
                k if k == count + 3 => {
                    self.pl_renaming = true;
                    self.ui.set_pl_rename_text(self.screens.playlists[mine].name.as_str().into());
                    self.ui.set_pl_renaming(true);
                }
                k if k == count + 4 => {
                    let id = self.screens.playlists[mine].id.clone();
                    if self.screens.delete(&id) {
                        self.pages_edited();
                        self.play_from_start();
                    }
                }
                _ => {}
            }
            self.refresh();
            return;
        }
        // the Sensors card's menus: the sensor or the card chosen
        if let Some(picks) = self.sensor_menu.clone() {
            self.close_menu();
            let mut ch = crate::sensors::choice();
            match picks.get(i).cloned().flatten() {
                Some(SensorPick::Cpu(d, l)) => ch.cpu = Some((d, l)),
                Some(SensorPick::Card(p, n)) => ch.card = Some((p, n)),
                None => return,
            }
            self.keep_sensors(ch);
            self.refresh();
            return;
        }
        // New screen layout's menu: a blank layout, or a copy of one, opened in the editor
        if let Some(made) = self.new_menu.clone() {
            self.close_menu();
            let at = match made.get(i).cloned().flatten() {
                Some(NewFrom::Scratch) => Some(self.screens.new_page()),
                Some(NewFrom::Copy(id)) => self.screens.index_of(&id).and_then(|k| self.copy_layout(k)),
                None => None,
            };
            if let Some(at) = at {
                self.pages_edited();
                self.pending_window.push(WindowAsk::OpenLayouts(at));
            }
            self.refresh();
            return;
        }
        // a layout card's menu: edit it, add it to the playlist (or keep it on the keyboard, on
        // the Onboard layout tab), copy it, or delete it once asked (Undo puts it back)
        if let Some(k) = self.lib_menu {
            self.close_menu();
            let Some(id) = self.screens.library.get(k).map(|p| p.id.clone()) else { return };
            let n = self.page.editing;
            match i {
                0 => self.pending_window.push(WindowAsk::OpenLayouts(k)),
                1 if self.ui.get_screen_closed() => {
                    self.screens.keep_onboard(n, Some(&id));
                    self.pages_edited();
                }
                1 => {
                    let shown = self.lcd.page;
                    let playing = self.screens.turns(n).get(shown).map(|e| e.key);
                    let end = self.screens.turns(n).len();
                    self.screens.add_turn(n, end, &id);
                    self.pages_edited();
                    self.keep_playing(playing, shown);
                }
                2 => {
                    if self.copy_layout(k).is_some() {
                        self.pages_edited();
                    }
                }
                4 => self.ask_delete(k),
                _ => {}
            }
            self.refresh();
            return;
        }
        // the holding bar's menu: a layout put on the end of the playlist
        if self.add_menu {
            self.close_menu();
            if let Some(id) = i.checked_sub(1).and_then(|k| self.screens.library.get(k)).map(|p| p.id.clone()) {
                let n = self.page.editing;
                let shown = self.lcd.page;
                let playing = self.screens.turns(n).get(shown).map(|e| e.key);
                let end = self.screens.turns(n).len();
                self.screens.add_turn(n, end, &id);
                self.pages_edited();
                self.keep_playing(playing, shown);
            }
            self.refresh();
            return;
        }
        // how long a turn shows, from the menu on its time
        if let Some(t) = self.time_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_menu_w(300.0);
            self.ui.set_pl_timing(-1);
            if let (Some(pick), Some(e)) = (i.checked_sub(1).and_then(|k| t.picks.get(k)), self.screens.turns_mut(self.page.editing).get_mut(t.turn)) {
                let (secs, stays) = match pick {
                    Some(secs) => (*secs, false),
                    None => (e.secs, true),
                };
                if (e.secs, e.stays) != (secs, stays) {
                    (e.secs, e.stays) = (secs, stays);
                    self.pages_edited();
                }
            }
            self.refresh();
            return;
        }
        // how a turn comes in, from the menu on its way bar
        if let Some(t) = self.way_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_menu_w(300.0);
            self.ui.set_pl_menu(-1);
            if let (Some(way), Some(e)) = (i.checked_sub(1).and_then(|k| crate::lcd_page::TRANSITIONS.get(k)), self.screens.turns_mut(self.page.editing).get_mut(t)) {
                if e.comes_in != *way {
                    e.comes_in = *way;
                    self.pages_edited();
                }
            }
            self.refresh();
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
        self.follow_profile();
        self.sync_aside();
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
                View::Keys => "Select a key on the keyboard or in the Key library to change what a key does.",
                View::Lighting => "Choose a lighting effect for this profile, then set the colors, speed and zones below.",
                View::Display => "Check the keyboard’s connection and which features the installed firmware supports.",
                View::Screen => "Choose what the keyboard’s screen shows while Ectodeck is running, and when it isn’t.",
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
            if self.sensors_seen.as_ref().is_none_or(|(t, _)| t.elapsed() >= Duration::from_secs(1)) {
                let v = self.sensors_view();
                self.sensors_seen = Some((Instant::now(), v));
            }
            let st = crate::ecto_settings::State {
                sensors: self.sensors_seen.as_ref().map(|(_, v)| v.clone()),
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
            // the default backlight at the right end of the tabs' row, with, while the layout
            // the screen shows has a color of its own, a line saying so: the playing layout on
            // the first tab, the onboard layout on the second; none while a page is edited
            let cells = if self.editor.is_some() {
                Vec::new()
            } else {
                let w = self.ui.get_under_w();
                let page = if self.ui.get_screen_closed() { if self.onboard_ready() { self.ob_page() } else { None } } else { self.shown() };
                let own = page.and_then(|p| p.light.map(|l| (p.name.clone(), l)));
                let picking = self.pick.as_ref().is_some_and(|p| p.ctl == Ctl::LcdColor);
                // the room the two tabs leave on their row: each tab its words and 18 either side, 4
                // between them, the open second tab's foot, then a gap (the names as app.slint has them)
                let tabs: f32 = ["Playlist", "Onboard layout"].iter().map(|t| crate::text::width(500, 13.0, t) + 36.0).sum();
                crate::display::screen_cells(&self.page, w, tabs + 4.0 + 10.0 + 24.0, own.as_ref().map(|(n, l)| (n.as_str(), *l)), picking)
            };
            self.show_cells(cells);
            self.refresh_onboard();
            self.refresh_editor();
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
        // the screen as the profile being edited has it, on the keyboard now (its own
        // status screen in the default backlight, whatever the playlist plays)
        if loaded && self.controlled {
            let mut want = screen_of(&self.page.draft()["lighting"]);
            if self.player.is_some() && !self.aside {
                let l = self.page_light([want.0[1], want.0[2], want.0[3]]);
                want.0[1..4].copy_from_slice(&l);
            } else if self.aside && self.onboard_ready() {
                // the keyboard by itself, as it lights its onboard layout: in the layout's own
                // color, or with the screen off, no light
                match self.screens.onboard_of(self.page.editing) {
                    Some(p) => {
                        if let Some(l) = p.light {
                            want.0[1..4].copy_from_slice(&l);
                        }
                    }
                    None => want.0[0] = 0,
                }
            }
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

    /// A turn's mark selected: the ways in, under it, the turn's own ticked.
    fn open_way_menu(&mut self, i: usize, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        let Some(e) = self.screens.turns(self.page.editing).get(i) else { return };
        let name = self.screens.page_of(e).map_or("The screen".to_string(), |p| p.name.clone());
        let mut rows = vec![MenuRow { kind: 1, label: format!("How {name} comes in").into(), tag: "".into(), tick: false, sub: "".into() }];
        for t in crate::lcd_page::TRANSITIONS {
            rows.push(MenuRow { kind: 0, label: t.name().into(), tag: "".into(), tick: t == e.comes_in, sub: "".into() });
        }
        // the only turn: nothing comes in after it, so its way in waits for another
        // (the menu as wide as the line, which doesn't wrap)
        let mut w = 200.0f32;
        if self.screens.turns(self.page.editing).len() == 1 {
            let note = "Plays once the playlist has a second layout.";
            w = w.max(crate::text::width(400, 13.0, note) + 34.0).ceil();
            rows.push(MenuRow { kind: 3, label: note.into(), tag: "".into(), tick: false, sub: "".into() });
        }
        self.close_menu();
        self.close_picker();
        let list: f32 = rows.iter().map(|r| if r.kind == 1 { 28.0 } else { 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
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
        self.way_menu = Some(i);
        self.ui.set_pl_menu(i as i32);
    }

    /// A turn's time selected: under it, the times a turn often has, its own among them, and
    /// staying until the layouts are switched; the turn's own ticked.
    fn open_time_menu(&mut self, i: usize, anchor: (f32, f32, f32), aw: f32) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        let Some(e) = self.screens.turns(self.page.editing).get(i) else { return };
        let name = self.screens.page_of(e).map_or("The screen".to_string(), |p| p.name.clone());
        let mut secs: Vec<u32> = TIMES.to_vec();
        if !secs.contains(&e.secs) {
            secs.push(e.secs);
            secs.sort();
        }
        let mut picks: Vec<Option<u32>> = secs.into_iter().map(Some).collect();
        picks.push(None);
        let row = |kind: i32, label: String, tick: bool| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: "".into() };
        let mut rows = vec![row(1, format!("How long {name} shows"), false)];
        for p in &picks {
            rows.push(match p {
                Some(s) => row(0, format!("{s} seconds"), !e.stays && e.secs == *s),
                None => row(0, STAYS.into(), e.stays),
            });
        }
        self.close_menu();
        self.close_picker();
        self.ui.set_menu_marks(true);
        let list: f32 = rows.iter().map(|r| if r.kind == 1 { 28.0 } else { 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        // (as wide as the longest line needs)
        let w = rows.iter().map(|r| crate::text::width(if r.kind == 1 { 500 } else { 400 }, 13.5, &r.label) + 58.0).fold(200.0f32, f32::max).ceil();
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
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
        self.time_menu = Some(TimeMenu { turn: i, aw, picks });
        self.ui.set_pl_timing(i as i32);
    }

    /// The panel's playlist selected: under it, every playlist (the one the
    /// profile plays ticked, each with the profiles that play it), then a new
    /// one, and this one renamed or deleted (never the last).
    fn open_pl_menu(&mut self, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        let n = self.page.editing;
        let mine = self.screens.of_profile(n);
        let row = |kind: i32, label: String, tick: bool, sub: String| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: sub.into() };
        let mut rows = vec![row(1, format!("{} plays", self.page.profile_name(n)), false, String::new())];
        for (k, l) in self.screens.playlists.iter().enumerate() {
            // (which profiles play it, by their names)
            let by: Vec<String> = self.screens.profiles_of(&l.id).iter().map(|p| self.page.profile_name(*p)).collect();
            let by = if by.is_empty() { "No profile".to_string() } else { by.join(", ") };
            rows.push(row(0, l.name.clone(), k == mine, by));
        }
        let name = self.screens.playlists[mine].name.clone();
        rows.push(row(2, String::new(), false, String::new()));
        rows.push(row(0, "New playlist".into(), false, String::new()));
        rows.push(row(0, format!("Rename {name}"), false, String::new()));
        rows.push(row(if self.screens.playlists.len() > 1 { 4 } else { 5 }, format!("Delete {name}"), false, String::new()));
        self.close_menu();
        self.close_picker();
        let list: f32 = rows.iter().map(|r| match r.kind { 1 => 28.0, 2 => 11.0, _ => 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        // (wide enough for the profiles' names beside each playlist's, up to a point)
        let widest = rows.iter().filter(|r| r.kind == 0).map(|r| crate::text::width(400, 13.5, &r.label) + if r.sub.is_empty() { 0.0 } else { 12.0 + crate::text::width(400, 12.0, &r.sub) }).fold(0.0, f32::max);
        self.ui.set_menu_w((widest + 74.0).clamp(248.0, 360.0));
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
        self.pl_menu = true;
        self.ui.set_pl_choosing(true);
    }

    /// The onboard layout's chooser selected: under it, the screen off and every screen
    /// layout, each that the keyboard can't show whole saying what stays blank, then the
    /// layout kept, to edit.
    fn open_ob_menu(&mut self, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        let n = self.page.editing;
        let kept = self.screens.onboard_of(n).map(|p| (p.id.clone(), p.name.clone()));
        let row = |kind: i32, label: String, tick: bool, sub: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: sub.into() };
        let mut rows = vec![row(1, format!("{}’s onboard layout", self.page.profile_name(n)), false, "")];
        rows.push(row(0, "Screen off".into(), kept.is_none(), ""));
        for p in &self.screens.library {
            rows.push(row(0, p.name.clone(), kept.as_ref().is_some_and(|(id, _)| *id == p.id), &crate::lcd_page::Gaps::of(p).tag()));
        }
        rows.push(row(2, String::new(), false, ""));
        match &kept {
            Some((_, name)) => rows.push(row(0, format!("Edit {name}"), false, "")),
            None => rows.push(row(5, "Edit".into(), false, "")),
        }
        self.close_menu();
        self.close_picker();
        let list: f32 = rows.iter().map(|r| match r.kind { 1 => 28.0, 2 => 11.0, _ => 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        // (as wide as the longest row needs, its note too, from 248 to 340)
        let w = rows.iter().map(|r| crate::text::width(400, 13.5, &r.label) + if r.sub.is_empty() { 0.0 } else { crate::text::width(400, 12.0, &r.sub) + 8.0 } + 64.0).fold(248.0f32, f32::max).min(340.0).ceil();
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
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
        self.ob_menu = true;
        self.ui.set_ob_choosing(true);
    }

    /// Whether the keyboard can keep an onboard layout: from build 5 of Ectodeck's
    /// firmware on (or, for the window's pictures of the tab, ECTODECK_NATIVE_ONBOARD).
    fn onboard_ready(&self) -> bool {
        crate::display::has(&self.firmware(), "screen.onboard") || std::env::var_os("ECTODECK_NATIVE_ONBOARD").is_some()
    }

    /// What the keyboard knows by itself, for the picture of its onboard layout: the time
    /// (its own clock, which Ectodeck sets from this computer's), and the name and number
    /// of the profile the tab shows; the rest as read here.
    fn onboard_readings(&self) -> crate::lcd_page::Readings {
        let mut r = self.readings.clone();
        r.now = Some(crate::lcd_page::Moment::now());
        if self.page.dev.is_some() {
            r.profile = Some(self.page.profile_name(self.page.editing));
            r.profile_number = Some(self.page.editing);
        }
        r
    }

    /// The layout the second tab's picture shows: one carried over the screen, before it
    /// is let go, or else the one the profile keeps on the keyboard; None for the screen off.
    fn ob_page(&self) -> Option<&crate::lcd_page::Page> {
        match &self.ob_over {
            Some(id) => self.screens.library.iter().find(|p| &p.id == id),
            None => self.screens.onboard_of(self.page.editing),
        }
    }

    /// The second tab's row under the screen: the chooser, named for the layout the
    /// profile keeps on the keyboard and as wide as that name, and the line beside it,
    /// which says what stays blank, if anything; on a keyboard without build 5 the row
    /// says what the keyboard needs instead.
    fn refresh_onboard(&mut self) {
        self.ui.set_ob_ready(self.onboard_ready());
        let kept = self.screens.onboard_of(self.page.editing);
        let name = kept.map_or_else(|| "Screen off".to_string(), |p| p.name.clone());
        let say = match kept {
            None => "The screen and its backlight stay off while Ectodeck isn’t running.".to_string(),
            Some(p) => crate::lcd_page::Gaps::of(p).line().unwrap_or_else(|| "While this tab is open, the keyboard shows this layout instead of the playlist.".to_string()),
        };
        self.ui.set_ob_chooser_w((crate::text::width(500, 13.0, &name) as f64 + 33.0).min(240.0).ceil() as f32);
        self.ui.set_ob_name(name.into());
        self.ui.set_ob_say(say.into());
        self.ui.set_ob_over(self.ob_over.is_some());
    }

    /// "New screen layout" selected, at the pointer: From scratch, or a copy of a layout.
    fn open_new_menu(&mut self, (x, y): (f32, f32)) {
        if self.card_shut.take().is_some_and(|(j, when)| j == self.screens.library.len() && when.elapsed() < Duration::from_millis(800)) {
            return;
        }
        let (rows, made) = new_rows(&self.screens.library);
        self.close_menu();
        self.close_picker();
        // (nothing in it is ticked)
        self.ui.set_menu_marks(false);
        let list: f32 = rows.iter().map(|r| match r.kind { 1 => 28.0, 2 => 11.0, _ => 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        let w = rows.iter().map(|r| crate::text::width(400, 13.5, &r.label) + 64.0).fold(200.0f32, f32::max).min(320.0).ceil();
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        self.ui.set_menu_w(w);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        let win = self.ui.get_window_height();
        let down = h <= win - y - 14.0;
        self.ui.set_menu_ax(x);
        self.ui.set_menu_ay(y);
        self.ui.set_menu_ah(0.0);
        self.ui.set_menu_top(if down { y + 2.0 } else { (y - 2.0 - h).max(14.0) });
        self.ui.set_menu_max(win * 0.7);
        self.ui.set_menu_open(true);
        self.new_menu = Some(made);
    }

    /// Layout `k` copied (the copy after it); the editor's window, open on a later layout,
    /// stays on that layout. The copy's place.
    fn copy_layout(&mut self, k: usize) -> Option<usize> {
        let at = self.screens.duplicate_page(k)?;
        if let Some(ed) = self.editor.as_mut().filter(|ed| ed.page >= at) {
            ed.page += 1;
        }
        self.refresh_editor();
        Some(at)
    }

    /// Where the readings with a choice of source come from now, for the editor's From
    /// line: the CPU sensor's and the card's names, looked for again every two seconds.
    fn reading_from(&mut self) -> crate::screen_edit::From {
        let ch = crate::sensors::choice();
        if self.from_seen.as_ref().is_none_or(|(t, c, _)| t.elapsed() >= Duration::from_secs(2) || *c != ch) {
            let root = std::path::Path::new("/");
            let all = crate::sensors::cpu_sensors(root);
            let sensor = ch.cpu_sensor(&all).map(|s| {
                let part = if s.own { crate::sensors::cpu_name(root) } else { crate::sensors::board_name(root).unwrap_or_else(|| "the motherboard".into()) };
                format!("{}, on the {part}", s.label)
            });
            let card = ch.card(&crate::sensors::cards(root)).map(|c| c.shown());
            self.from_seen = Some((Instant::now(), ch.clone(), crate::screen_edit::From { sensor, fastest: ch.fastest, card }));
        }
        self.from_seen.as_ref().unwrap().2.clone()
    }

    /// What the Sensors card shows: the sensors that read the processor and the graphics
    /// cards on this computer, each read now, and the choices.
    fn sensors_view(&mut self) -> crate::ecto_settings::Sensors {
        let root = std::path::Path::new("/");
        let cpu = crate::sensors::cpu_sensors(root).into_iter().map(|s| (s.read(), s)).map(|(t, s)| (s, t)).collect();
        let cards = crate::sensors::cards(root);
        let temps = self.reader.get_or_insert_with(crate::lcd_stats::Stats::here).card_temps(&cards);
        crate::ecto_settings::Sensors {
            cpu,
            cpu_name: crate::sensors::cpu_name(root),
            board: crate::sensors::board_name(root),
            cores: crate::sensors::cpu_cores(root),
            cards: cards.into_iter().zip(temps).collect(),
            choice: crate::sensors::choice(),
        }
    }

    /// A Sensors choice made: kept in the settings, and read from at once.
    fn keep_sensors(&mut self, ch: crate::sensors::Choice) {
        if let Err(e) = self.settings.set("sensors", ch.to_json()) {
            self.status(format!("The sensor choice was not saved: {e}"), 2);
            return;
        }
        crate::sensors::choose(ch);
        self.sensors_seen = None;
        // (the editor's and the playlist's pictures read again)
        self.read_at = None;
    }

    /// The Sensors card's menus, under their dropdown: every sensor that reads the
    /// processor, under the part it belongs to, or every graphics card; each with its
    /// reading now, the one chosen ticked.
    fn open_sensor_menu(&mut self, c: Ctl, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        let v = self.sensors_view();
        let row = |kind: i32, label: String, tick: bool, sub: String| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: sub.into() };
        let (mut rows, mut picks): (Vec<MenuRow>, Vec<Option<SensorPick>>) = (Vec::new(), Vec::new());
        if c == Ctl::CpuSensor {
            let all: Vec<crate::sensors::CpuSensor> = v.cpu.iter().map(|(s, _)| s.clone()).collect();
            let chosen = v.choice.cpu_sensor(&all).cloned();
            for (own, head) in [(true, v.cpu_name.clone()), (false, v.board.clone().unwrap_or_else(|| "The motherboard".into()))] {
                let group: Vec<&(crate::sensors::CpuSensor, Option<f64>)> = v.cpu.iter().filter(|(s, _)| s.own == own).collect();
                if group.is_empty() {
                    continue;
                }
                rows.push(row(1, head, false, String::new()));
                picks.push(None);
                for (s, t) in group {
                    rows.push(row(0, s.label.clone(), chosen.as_ref() == Some(s), t.map_or(String::new(), |t| format!("{}°", t.round()))));
                    picks.push(Some(SensorPick::Cpu(s.driver.clone(), s.label.clone())));
                }
            }
        } else {
            let all: Vec<crate::sensors::Card> = v.cards.iter().map(|(k, _)| k.clone()).collect();
            let chosen = v.choice.card(&all).cloned();
            for (k, t) in &v.cards {
                rows.push(row(0, k.shown(), chosen.as_ref() == Some(k), t.map_or(String::new(), |t| format!("{t}°"))));
                picks.push(Some(SensorPick::Card(k.pci.clone(), k.name.clone())));
            }
        }
        if rows.is_empty() {
            return;
        }
        self.close_menu();
        self.close_picker();
        self.ui.set_menu_marks(true);
        let list: f32 = rows.iter().map(|r| if r.kind == 1 { 28.0 } else { 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        // (as wide as its dropdown, or as its longest name and reading need)
        let w = rows.iter().map(|r| crate::text::width(400, 13.5, &r.label) + crate::text::width(400, 12.0, &r.sub) + 76.0).fold(300.0f32, f32::max).min(460.0).ceil();
        self.ui.set_menu_w(w);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        let (ax, ay, ah) = anchor;
        let win = self.ui.get_window_height();
        let (below, above) = (win - ay - ah - 14.0, ay - 70.0);
        let down = h <= below || below >= above;
        let room = if down { below } else { above };
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_ay(ay);
        self.ui.set_menu_ah(ah);
        self.ui.set_menu_top(if down { ay + ah + 6.0 } else { ay - 6.0 - h.min(room) });
        self.ui.set_menu_max(if h > room { room } else { win * 0.7 });
        self.ui.set_menu_open(true);
        self.sensor_menu = Some(picks);
    }

    /// Layout `k`'s Delete chosen from its card's menu: first the question, over the window,
    /// saying what goes with the layout.
    fn ask_delete(&mut self, k: usize) {
        let Some(page) = self.screens.library.get(k) else { return };
        let says = self.screens.delete_says(k, self.onboard_ready(), |n| self.page.profile_name(n));
        self.ui.set_confirm_title(format!("Delete {}?", page.name).into());
        self.ui.set_confirm_body(says.into());
        self.ui.set_confirm_do("Delete layout".into());
        self.deleting = Some(page.id.clone());
        self.ui.set_confirm_open(true);
    }

    /// Layout `k` deleted, Delete answered: it comes off the playlists (and the keyboard),
    /// and the editor's window, open on a later layout or on this one, follows.
    fn delete_layout(&mut self, k: usize) {
        if !self.screens.delete_page(k) {
            return;
        }
        if let Some(ed) = &mut self.editor {
            if ed.page > k || (ed.page == k && ed.page >= self.screens.library.len()) {
                ed.page = ed.page.saturating_sub(1);
            }
            ed.chosen = None;
            if self.screens.library.is_empty() {
                self.editor = None;
            }
        }
        self.pages_edited();
        self.refresh_editor();
    }

    /// The playlist's holding bar selected: under it, every screen layout,
    /// to put one on the end of the playlist.
    /// A layout card's own menu, right-clicked, at the pointer: Edit, Add to the playlist
    /// (on the Onboard layout tab, Keep on the keyboard), Duplicate, and Delete.
    fn open_lib_menu(&mut self, k: usize, at: (f32, f32)) {
        if self.card_shut.take().is_some_and(|(j, when)| j == k && when.elapsed() < Duration::from_millis(800)) {
            return;
        }
        let Some(name) = self.screens.library.get(k).map(|p| p.name.clone()) else { return };
        let row = |kind: i32, label: String| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into() };
        let second = if self.ui.get_screen_closed() {
            format!("Keep {name} on the keyboard")
        } else {
            format!("Add {name} to {}", self.screens.playlist(self.page.editing).name)
        };
        let rows = vec![row(0, format!("Edit {name}")), row(0, second), row(0, format!("Duplicate {name}")), row(2, String::new()), row(4, format!("Delete {name}…"))];
        self.close_menu();
        self.close_picker();
        // (nothing in it is ticked)
        self.ui.set_menu_marks(false);
        let list: f32 = rows.iter().map(|r| if r.kind == 2 { 11.0 } else { 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        let w = rows.iter().map(|r| crate::text::width(400, 13.5, &r.label) + 64.0).fold(200.0f32, f32::max).min(320.0).ceil();
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        self.ui.set_menu_w(w);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        // (from the pointer, down when it fits, else up)
        let (x, y) = at;
        let win = self.ui.get_window_height();
        let down = h <= win - y - 14.0;
        self.ui.set_menu_ax(x);
        self.ui.set_menu_ay(y);
        self.ui.set_menu_ah(0.0);
        self.ui.set_menu_top(if down { y + 2.0 } else { (y - 2.0 - h).max(14.0) });
        self.ui.set_menu_max(win * 0.7);
        self.ui.set_menu_open(true);
        self.lib_menu = Some(k);
    }

    fn open_add_menu(&mut self, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into() };
        let mut rows = vec![row(1, "Add a screen layout")];
        for p in &self.screens.library {
            rows.push(row(0, &p.name));
        }
        self.close_menu();
        self.close_picker();
        self.ui.set_menu_marks(false);
        let list: f32 = rows.iter().map(|r| if r.kind == 1 { 28.0 } else { 33.0 }).sum();
        let h = 6.0 + list + 6.0;
        // (as wide as the longest name needs, to 300)
        let w = rows.iter().map(|r| crate::text::width(400, 13.5, &r.label) + 64.0).fold(200.0f32, f32::max).min(300.0).ceil();
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
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
        self.add_menu = true;
        self.ui.set_pl_adding(true);
    }

    /// The playlist's name changed in place, kept or let go; then the
    /// chooser again.
    fn end_rename(&mut self, keep: bool, typed: &str) {
        if !std::mem::take(&mut self.pl_renaming) {
            return;
        }
        self.ui.set_pl_renaming(false);
        // (its field gone, nothing is being typed in)
        slint::ComponentHandle::global::<crate::Typing>(&self.ui).set_on(false);
        if keep {
            let n = self.page.editing;
            let (id, was) = { let l = self.screens.playlist(n); (l.id.clone(), l.name.clone()) };
            self.screens.rename(&id, typed);
            if self.screens.playlist(n).name != was {
                self.pages_edited();
            }
        }
        self.refresh();
    }

    /// The playlist the page's profile plays, on the screen from its first turn.
    fn play_from_start(&mut self) {
        self.tell_page();
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Show(0));
        }
        self.lcd.page = 0;
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
        self.tell_page();
        self.tell_context();
    }

    /// A page edited: the keyboard shows the change at once, and it is saved
    /// once the editing pauses.
    fn pages_edited(&mut self) {
        self.pages_changed = Some(Instant::now());
        self.tell_page();
    }

    /// The pages the screen plays in turn, to the player: the playlist's
    /// turns, or, while a page is edited, that page alone.
    fn tell_page(&mut self) {
        let pages = match self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)) {
            Some(p) => vec![p.clone()],
            None => self.screens.played(self.page.editing),
        };
        // an empty playlist: the screen blank, in the profile's default backlight
        let pages = if pages.is_empty() { vec![crate::lcd_page::Page::blank()] } else { pages };
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Pages(pages));
        }
    }

    /// The screen plays the playlist of the profile the page shows, as it
    /// shows that profile's screen light: when another profile is chosen, in
    /// Ectodeck or on the keyboard, the screen plays that profile's playlist
    /// from its first turn. (Not while a page is edited: the keyboard stays
    /// on that page.)
    fn follow_profile(&mut self) {
        let a = self.page.editing;
        if a == self.played_by || self.editor.is_some() {
            return;
        }
        self.played_by = a;
        self.play_from_start();
    }

    /// After the playlist changed: the turn that was playing (by its key)
    /// goes on playing where it now is; taken off, the turn now in its place.
    fn keep_playing(&mut self, playing: Option<u64>, was: usize) {
        let now = playing.and_then(|k| self.screens.turns(self.page.editing).iter().position(|e| e.key == k));
        let at = now.unwrap_or(was.min(self.screens.turns(self.page.editing).len().saturating_sub(1)));
        if now != Some(was) {
            if let Some(pl) = &self.player {
                pl.tell(crate::lcd_player::Control::Show(at));
            }
            // (marked at once, not a moment later when the player says so)
            self.lcd.page = at;
        }
    }

    /// What a page may show besides the computer's readings, to the player:
    /// the profile the keyboard is using, and the weather outside.
    fn tell_context(&mut self) {
        let profile = self.page.dev.as_ref().map(|_| (self.page.active(), self.page.profile_name(self.page.active())));
        let outside = self.weather.report().map(|r| (r.temperature as f64, crate::ecto_settings::sky(r.code)));
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Context(profile, outside));
        }
    }

    /// The Library and the playlist into Ectodeck's settings, if they
    /// changed since last saved.
    fn save_pages(&mut self) {
        self.note_changes();
        if self.screens == self.screens_saved {
            return;
        }
        let v = self.screens.to_json();
        let saved = self
            .settings
            .set("devices.z10.screen.pages", v["pages"].clone())
            .and_then(|_| self.settings.set("devices.z10.screen.playlists", v["playlists"].clone()))
            .and_then(|_| self.settings.set("devices.z10.screen.plays", v["plays"].clone()))
            .and_then(|_| self.settings.set("devices.z10.screen.onboard", v["onboard"].clone()));
        match saved {
            Ok(()) => {
                self.screens_saved = self.screens.clone();
                // said as a profile's saving is, with Undo beside it
                self.status("Saved", 1);
                self.ui.set_undo_shown(!self.undo.is_empty());
            }
            Err(e) => self.status(format!("The screen layouts and playlists were not saved: {e}"), 2),
        }
        self.pages_changed = None;
    }

    /// This computer's readings now, for the editor's picture of the page.
    fn read_now(&mut self) {
        let mut r = self.reader.get_or_insert_with(|| {
            let mut s = crate::lcd_stats::Stats::here();
            s.cpu_load();
            s
        }).read();
        r.now = Some(crate::lcd_page::Moment::now());
        r.profile = self.page.dev.as_ref().map(|_| self.page.profile_name(self.page.active()));
        r.profile_number = self.page.dev.as_ref().map(|_| self.page.active());
        r.outside = self.weather.report().map(|w| (w.temperature as f64, crate::ecto_settings::sky(w.code)));
        self.readings = r;
        self.read_at = Some(Instant::now());
    }

    /// The screen's light while Ectodeck plays it: the page's own, or the
    /// profile's.
    fn page_light(&self, profile: [u8; 3]) -> [u8; 3] {
        self.shown().and_then(|p| p.light).unwrap_or(profile)
    }

    /// The page on the keyboard's screen: the one being edited, or the
    /// page of the turn the playlist has come to.
    fn shown(&self) -> Option<&crate::lcd_page::Page> {
        match &self.editor {
            Some(ed) => self.screens.library.get(ed.page),
            None => self.screens.turns(self.page.editing).get(self.lcd.page).and_then(|e| self.screens.page_of(e)),
        }
    }

    /// The playlist's turn on the keyboard's screen.
    fn shown_page(&self) -> usize {
        self.lcd.page
    }

    /// The player said what the screen shows now.
    pub fn screen_news(&mut self, n: crate::lcd_player::News) {
        let state_changed = n.state != self.lcd.state || n.sequence != self.lcd.sequence || n.page != self.lcd.page;
        // a page coming on, or playing again after a pause, has its whole time
        if n.page != self.lcd.page || (n.state == crate::lcd_player::State::Playing && self.lcd.state != crate::lcd_player::State::Playing) {
            self.page_from = Instant::now();
        }
        self.lcd = n;
        self.set_playhead();
        if self.page.view == View::Screen && self.ui.get_place() == "z10" {
            if state_changed {
                self.refresh();
            } else {
                self.draw_screen();
            }
        }
    }

    /// The playhead, where the player last said the playlist stands: moving
    /// on its own from there while the playlist plays, still while it is
    /// paused; not shown for a single turn, while a script's screen shows or
    /// while a page is edited.
    fn set_playhead(&mut self) {
        use crate::lcd_player::State;
        let n = &self.lcd;
        let shown = self.controlled && self.editor.is_none() && n.sequence.is_none() && matches!(n.state, State::Playing | State::Paused) && self.screens.turns(self.page.editing).len() > 1;
        // a turn that stays until the layouts are switched has no end to come to: the
        // playhead stands at its bar's end, by its infinity mark
        let stays = !n.turning && n.length.is_infinite();
        let len = if stays { 1.0 } else { n.length.max(0.001) };
        let tick = self.ui.get_pl_tick();
        self.ui.set_pl_head(shown);
        self.ui.set_pl_in(n.turning);
        self.ui.set_pl_len((len * 1000.0).round().max(1.0) as i64);
        self.ui.set_pl_t0(tick - (n.elapsed.min(len) * 1000.0).round() as i64);
        self.ui.set_pl_moving(shown && n.state == State::Playing && !stays);
        self.ui.set_pl_p0(if stays { 1.0 } else { (n.elapsed / len).clamp(0.0, 1.0) as f32 });
    }

    /// Whether the keyboard shows its own status screen for the page: while the Screen
    /// tab is open on it, in sight, and no page is edited, on a keyboard Ectodeck controls.
    /// Then the playlist stands aside, as it does for good once Ectodeck quits.
    fn status_showing(&self) -> bool {
        self.controlled
            && self.page.dev.is_some()
            && !self.hidden
            && self.editor.is_none()
            && self.ui.get_place() == "z10"
            && self.page.view == View::Screen
            && self.ui.get_screen_closed()
    }

    /// The player told whether to stand aside, as the page now has it.
    fn sync_aside(&mut self) {
        let aside = self.status_showing();
        if aside == self.aside {
            return;
        }
        self.aside = aside;
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Aside(aside));
        }
    }

    /// The window closed into the tray, or shown again.
    pub fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        self.sync_aside();
    }

    /// Ectodeck closing: the screen handed back to the keyboard's own screens.
    pub fn quit_screen(&mut self) {
        if let Some(p) = self.player.take() {
            p.quit(true);
        }
    }

    /// The screen layout editor's window opened on layout `i` of the Library (the window
    /// itself made by the window's side): the keyboard shows the layout and stays on it
    /// while the layout is edited.
    pub fn open_layouts(&mut self, i: usize) {
        self.close_menu();
        if !self.layouts_shown {
            self.edit_from = self.lcd.page;
            self.layouts_shown = true;
        }
        let i = i.min(self.screens.library.len().saturating_sub(1));
        let grid = self.editor.as_ref().map(|e| e.grid).unwrap_or_else(|| crate::uistate::get("screen_grid").and_then(|v| v.as_bool()).unwrap_or(true));
        self.editor = (!self.screens.library.is_empty()).then_some(crate::screen_edit::Editor { page: i, chosen: None, grid });
        self.read_now();
        self.hold_edited();
        self.show_layouts();
        if let Some(l) = &self.layouts {
            l.reveal(i);
        }
        self.refresh();
    }

    /// The keyboard's screen while the editor is open: the layout open, and the screen
    /// held on it.
    fn hold_edited(&mut self) {
        self.tell_page();
        if let (Some(pl), true) = (&self.player, self.editor.is_some()) {
            pl.tell(crate::lcd_player::Control::Hold(Some(0)));
        }
    }

    /// The editor's window closed: the layouts saved, and the screen plays the playlist
    /// again, from the turn it was on.
    pub fn layouts_closed(&mut self) {
        if !std::mem::take(&mut self.layouts_shown) {
            return;
        }
        self.editor = None;
        self.save_pages();
        self.tell_page();
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Show(self.edit_from.min(self.screens.turns(self.page.editing).len().saturating_sub(1))));
            pl.tell(crate::lcd_player::Control::Hold(None));
        }
        self.refresh();
    }

    /// The editor window's asks, carried out on the layouts and the screen.
    pub fn act_layouts(&mut self) -> Vec<WindowAsk> {
        if self.layouts.is_none() {
            return Vec::new();
        }
        let avail = crate::lcd_page::available(&self.readings, self.weather.report().is_some());
        let light = self.layouts_light();
        let t = self.edit_t();
        let (onboard, profiles, from) = (self.onboard_ready(), crate::lcd_page::PROFILES.map(|n| self.page.profile_name(n)), self.reading_from());
        let Some(l) = &mut self.layouts else { return Vec::new() };
        let mut cx = crate::layout_page::Cx { screens: &mut self.screens, editor: &mut self.editor, readings: &self.readings, avail: &avail, light, t, onboard, profiles, from };
        let out = l.act(&mut cx);
        l.show(&cx);
        if out.closed {
            self.layouts_closed();
        } else {
            if out.edited {
                self.pages_edited();
            }
            if out.opened {
                if let Some(i) = self.editor.as_ref().map(|e| e.page) {
                    if let Some(l) = &self.layouts {
                        l.reveal(i);
                    }
                }
                self.hold_edited();
            }
            if out.edited || out.opened {
                self.refresh();
            }
        }
        out.window
    }

    /// The editor's window shown as the layouts are now (each second too, for its readings).
    pub fn show_layouts(&mut self) {
        if self.layouts.is_none() {
            return;
        }
        let avail = crate::lcd_page::available(&self.readings, self.weather.report().is_some());
        let light = self.layouts_light();
        let t = self.edit_t();
        let (onboard, profiles, from) = (self.onboard_ready(), crate::lcd_page::PROFILES.map(|n| self.page.profile_name(n)), self.reading_from());
        let Some(l) = &mut self.layouts else { return };
        let cx = crate::layout_page::Cx { screens: &mut self.screens, editor: &mut self.editor, readings: &self.readings, avail: &avail, light, t, onboard, profiles, from };
        l.show(&cx);
    }

    /// Seconds since the layout open in the editor came up (another layout opened starts
    /// the count again), which its scrolling words are timed from.
    fn edit_t(&mut self) -> f64 {
        let Some(id) = self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)).map(|p| p.id.clone()) else {
            self.edit_clock = None;
            return 0.0;
        };
        if self.edit_clock.as_ref().is_none_or(|(open, _)| *open != id) {
            self.edit_clock = Some((id, Instant::now()));
        }
        self.edit_clock.as_ref().map_or(0.0, |(_, since)| since.elapsed().as_secs_f64())
    }

    /// Whether the layout open in the editor has words scrolling now, so its pictures
    /// change between readings.
    fn edit_moving(&self) -> bool {
        self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)).is_some_and(|p| crate::lcd_page::moving(p, &self.readings))
    }

    /// The default backlight of the profile shown, for the editor's window: its
    /// brightness and color (until the keyboard's state has come, the screen's white).
    fn layouts_light(&self) -> [u8; 4] {
        if self.page.dev.is_none() {
            return [255, 255, 255, 255];
        }
        screen_of(&self.page.draft()["lighting"]).0
    }

    /// Whether the editor's window changed under the pointer since it last moved (a block
    /// dropped or deleted where it rests), so what is under it is asked for again.
    pub fn layouts_rehover(&mut self) -> bool {
        self.layouts.as_mut().is_some_and(|l| l.take_rehover())
    }

    /// Whether words are being typed in the editor's window (its name, a Text block's words).
    pub fn typing_in_layouts(&self) -> bool {
        self.layouts.as_ref().is_some_and(|l| slint::ComponentHandle::global::<crate::Typing>(&l.ui).get_on())
    }

    /// What the screen is doing when the strip can't show it, said on the
    /// line over the screen: another program has it, or it isn't there.
    fn screen_status(&self) -> String {
        use crate::lcd_player::State;
        match (self.lcd.state, self.lcd.sequence) {
            (_, _) if !self.controlled => "Ectodeck leaves this keyboard to other programs.".into(),
            (State::Other, _) => "Another program is using the screen. Ectodeck plays it once that stops.".into(),
            (State::Missing, _) => "Ectodeck can’t reach the keyboard’s screen.".into(),
            // (the strip shows what plays, paused or not, and a script's screen)
            _ => String::new(),
        }
    }

    /// The playlist's panel: its chooser as wide as the playlist's name, the
    /// strip after it, each turn at its place (room made for a screen being
    /// carried, the strip fitted to its width unless a turn's end is being
    /// dragged); and the Library's cards. `profile` is the profile's screen light.
    fn refresh_playlist(&mut self, profile: [u8; 3], level: u8) {
        use crate::lcd_player::State;
        let who = self.page.editing;
        let played = self.screens.played(who);
        let n = played.len();
        let sc = self.scale.max(1.0);
        let px = |w: f32| (w * sc).round() as u32;
        let lv = level as f32 / 255.0;
        let shown = self.shown_page();
        // the turn on the screen is marked while Ectodeck plays the screen, playing or paused
        let marked = self.controlled && self.editor.is_none() && matches!(self.lcd.state, State::Playing | State::Paused);
        // the panel's head: the playlist's name in its chooser (as wide as the name needs,
        // to 200) or the field it is renamed in; the strip after it, to 82 short of the
        // panel's end, where the round starts again and then Pause
        let under_w = self.ui.get_under_w() as f64;
        let name = self.screens.playlist(who).name.clone();
        let chooser = (crate::text::width(500, 13.0, &name) as f64 + 33.0).min(200.0).ceil();
        let track_x = 16.0 + if self.pl_renaming { 195.0 } else { chooser } + 14.0;
        let track_w = (under_w - 82.0 - track_x).max(120.0);
        self.ui.set_pl_name(name.as_str().into());
        self.ui.set_pl_chooser_w(chooser as f32);
        self.ui.set_pl_track_x(track_x as f32);
        self.ui.set_pl_track_w(track_w as f32);
        // the turns shown and the room, in order: (the turn, or None for the room; its time)
        let carried = self.carry.as_ref().filter(|c| c.from_strip).map(|c| c.index);
        let mut items: Vec<(Option<usize>, f64)> = (0..n).filter(|i| Some(*i) != carried).map(|i| (Some(i), played[i].secs as f64)).collect();
        if let Some(at) = self.carry.as_ref().and_then(|c| c.slot) {
            items.insert(at.min(items.len()), (None, self.carry.as_ref().map_or(8.0, |c| c.secs as f64)));
        }
        // the empty holding bar at the end, its room kept from the turns' own; the whole strip
        // while the playlist is empty (and none while a carried layout's room takes its place)
        let hold = if n > 0 { HOLD_W } else { 0.0 };
        let fit_w = if n > 0 { track_w - HOLD_W - BETWEEN } else { track_w };
        let (pps, way, spans) = lay_strip(&items.iter().map(|x| x.1).collect::<Vec<_>>(), self.sizing.map(|s| (s.2, self.strip_way)), fit_w);
        self.strip_pps = pps;
        self.strip_way = way;
        self.ui.set_pl_way_w(way as f32);
        let end = spans.last().map_or(0.0, |(x, w)| x + w);
        let (hold_x, hold_w) = match (n, items.is_empty()) {
            (0, true) => (0.0, track_w - BETWEEN),
            (0, false) => (0.0, 0.0),
            _ => (end + BETWEEN, hold),
        };
        self.ui.set_pl_hold_x(hold_x as f32);
        self.ui.set_pl_hold_w(hold_w as f32);
        let room = items.iter().zip(&spans).find(|(it, _)| it.0.is_none()).map(|(_, sp)| *sp);
        self.ui.set_pl_slot_x(room.map_or(0.0, |r| r.0) as f32);
        self.ui.set_pl_slot_w(room.map_or(0.0, |r| r.1) as f32);
        self.strip_spans = items.iter().zip(&spans).filter(|(it, _)| it.0.is_some()).map(|(_, sp)| *sp).collect();
        let mut place: Vec<Option<(f64, f64)>> = vec![None; n];
        for (it, sp) in items.iter().zip(&spans) {
            if let Some(i) = it.0 {
                place[i] = Some(*sp);
            }
        }
        let old: HashMap<i32, (f32, f32)> = (0..self.blocks.row_count()).filter_map(|i| self.blocks.row_data(i)).map(|b| (b.key, (b.x, b.w))).collect();
        let mut rows = Vec::with_capacity(n);
        for (i, p) in played.iter().enumerate() {
            let e = &self.screens.turns(who)[i];
            let key = e.key as i32;
            // (a turn being carried keeps its place, unseen, until it lands)
            let (x, w) = place[i].map(|(x, w)| (x as f32, w as f32)).or(old.get(&key).copied()).unwrap_or((0.0, 0.0));
            rows.push(crate::PlBlock {
                key,
                x,
                w,
                name: p.name.as_str().into(),
                on: marked && i == shown,
                away: place[i].is_none(),
                secs: if n == 1 {
                    "Shows all the time".into()
                } else if e.stays {
                    STAYS.into()
                } else {
                    format!("{} s", e.secs).into()
                },
                stays: e.stays,
                way: crate::lcd_page::TRANSITIONS.iter().position(|t| *t == e.comes_in).unwrap_or(0) as i32,
                way_name: e.comes_in.name().into(),
            });
        }
        sync_blocks(&self.blocks, rows);
        self.ui.set_pl_single(n == 1);
        self.ui.set_pl_empty(n == 0);
        // a script's screen on the keyboard: the strip waits
        let script = match self.lcd.sequence {
            Some("launch") => "A game is starting",
            Some(_) => "A game just closed",
            None => "",
        };
        self.ui.set_pl_dim(!script.is_empty() && self.controlled);
        self.ui.set_pl_script(script.into());
        // the Library: every page, its picture two pixels to a dot, its name, and what it
        // shows in at most two lines of its card's room
        let card_w = if self.ui.get_shelf_two() { (under_w - 32.0 - 8.0) / 2.0 } else { under_w - 32.0 };
        // (the words end short of the card's right edge, where Edit shows under the pointer)
        let words_w = (card_w - 358.0).max(60.0) as f32;
        self.pl_thumbs.resize(self.screens.library.len(), None);
        let mut cards = Vec::with_capacity(self.screens.library.len());
        for (k, p) in self.screens.library.iter().enumerate() {
            let rgb = crate::z10::as_lit(p.light.unwrap_or(profile));
            let key: PicKey = (crate::lcd_page::render(p, &self.readings), rgb, level, px(260.0));
            let pic = match &self.pl_thumbs[k] {
                Some((had, img)) if *had == key => img.clone(),
                _ => {
                    let img = crate::lcd_draw::picture(&key.0, key.1, lv, px(260.0), px(52.0));
                    self.pl_thumbs[k] = Some((key, img.clone()));
                    img
                }
            };
            let (line1, line2) = two_lines(&p.about(), words_w);
            cards.push(crate::PlPage {
                name: p.name.as_str().into(),
                line1: line1.into(),
                line2: line2.into(),
                pic,
                field: crate::lcd_draw::field(rgb, lv, px(260.0)),
                on: false,
                now: false,
                paused: false,
                removable: false,
            });
        }
        show_rows(self.ui.get_lib_pages(), |m| self.ui.set_lib_pages(m), cards);
        // the layout asked about gone another way (from the editor's window): no question left
        if self.deleting.as_ref().is_some_and(|id| self.screens.index_of(id).is_none()) {
            self.deleting = None;
            self.ui.set_confirm_open(false);
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
        // the second tab: what the keyboard shows by itself, whatever the playlist plays. Its
        // onboard layout, from build 5 of its firmware on (Some, or Some(None) for the screen
        // off); before that, EVGA's status screen, in the default backlight
        let closed = self.ui.get_screen_closed() && self.editor.is_none();
        let onboard = (closed && self.onboard_ready()).then(|| self.ob_page().cloned());
        let profile = [light[1], light[2], light[3]];
        let lit = match &onboard {
            Some(Some(p)) => p.light.unwrap_or(profile),
            // (no light at all)
            Some(None) => [0, 0, 0],
            None if closed => profile,
            None => self.page_light(profile),
        };
        // the light as the keyboard's screen shows it, bled as its backlight bleeds (and none
        // with the screen off)
        let level = if matches!(onboard, Some(None)) { 0 } else { light[0] };
        let rgb = crate::z10::as_lit(lit);
        let sc = self.scale.max(1.0);
        let px = |w: f32| (w * sc).round() as u32;
        // editing: the page as edited, read here; otherwise what the keyboard shows
        let t = self.edit_t();
        let rows = match self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)) {
            Some(p) => crate::lcd_page::render_at(p, &self.readings, t),
            None => match &onboard {
                Some(Some(p)) => crate::lcd_page::render_onboard(p, &self.onboard_readings()),
                Some(None) => vec![String::new(); 4],
                // (nothing sends the keyboard the time, so its clock's row is blank)
                None if closed => {
                    let d = self.page.draft();
                    crate::lcd_page::status_rows(d["lighting"]["lcd"]["apps"].as_u64().unwrap_or(0), self.page.editing, d["name"].as_str().unwrap_or(""), None)
                }
                None => self.lcd.rows.clone(),
            },
        };
        // (as large as its box leaves it)
        let (lw, lh) = (self.ui.get_lcd_w(), self.ui.get_lcd_h());
        let key = (rows.clone(), rgb, level, px(lw));
        if self.lcd_drawn.as_ref() != Some(&key) {
            let lv = level as f32 / 255.0;
            self.ui.set_lcd_big(crate::lcd_draw::picture(&rows, rgb, lv, px(lw), px(lh)));
            self.ui.set_lcd_field(crate::lcd_draw::field(rgb, lv, px(lw)));
            self.lcd_drawn = Some(key);
        }
        self.refresh_playlist([light[1], light[2], light[3]], light[0]);
    }

    /// The layout editor's window follows what the main window changes (the
    /// profile's default backlight, the layouts).
    fn refresh_editor(&mut self) {
        if self.layouts_shown {
            self.show_layouts();
        }
    }
}

/// What a picture of the screen was drawn from: its rows, its light and
/// brightness, and its width in pixels.
type PicKey = (Vec<String>, [u8; 3], u8, u32);

/// A screen carried to the strip: from the strip (turn `index`) or the
/// Library (page `index`); its page and time; where it would land among the
/// turns that stay (None, nowhere: let go, nothing changes); where a turn
/// carried from the strip goes back to when the pointer leaves the strip.
#[derive(Clone, Debug)]
struct Carry {
    from_strip: bool,
    index: usize,
    page: String,
    secs: u32,
    slot: Option<usize>,
    home: Option<usize>,
}

/// The menu of times open on a turn's time: the turn, how wide the time it hangs from is,
/// and what each row after the heading gives the turn (its seconds, or None: it stays
/// until the layouts are switched).
#[derive(Clone, Debug)]
struct TimeMenu {
    turn: usize,
    aw: f32,
    picks: Vec<Option<u32>>,
}

/// The times a turn often has, in its menu of times (a turn's own, dragged to another,
/// joins them in order), and the choice to stay until the layouts are switched, as the
/// menu and the turn's time say it.
const TIMES: [u32; 5] = [5, 8, 15, 30, 60];
const STAYS: &str = "Until you switch layouts";

/// A turn's way bar: as long as the strip has room for, between these; the
/// hairline joining it to its screen's bar; the room between one turn and the
/// next; and the empty holding bar's length, at the strip's end.
const WAY_W: (f64, f64) = (18.0, 26.0);
const JOIN: f64 = 1.0;
const BETWEEN: f64 = 6.0;
const HOLD_W: f64 = 72.0;

/// The turns laid out across a strip `width` long: each its way bar, the
/// hairline, then its screen's bar as long as its time, at one scale (pixels
/// a second) that fits them all. `fixed` keeps a scale and a way bar's length
/// instead (while a turn's end is dragged: the strip refits once it is let
/// go). The scale, the way bars' length, and each screen's bar's start and length.
fn lay_strip(secs: &[f64], fixed: Option<(f64, f64)>, width: f64) -> (f64, f64, Vec<(f64, f64)>) {
    let n = secs.len().max(1) as f64;
    let total = secs.iter().sum::<f64>().max(1.0);
    let way = fixed.map_or(((width - 24.0 * n) / n - JOIN - BETWEEN).floor().clamp(WAY_W.0, WAY_W.1), |f| f.1);
    let (pre, per) = (way + JOIN, way + JOIN + BETWEEN);
    let fit = ((width - per * n) / total).max(0.05);
    // (while an end is dragged, the scale it began with holds, so the end follows the
    // pointer; but the strip never runs past its room: there, it is fitted again, every
    // turn narrower, and the time still follows the pointer)
    let pps = fixed.map_or(fit, |f| f.0.min(fit));
    let mut x = pre;
    let spans = secs
        .iter()
        .map(|s| {
            let at = (x, s * pps);
            x += s * pps + per;
            at
        })
        .collect();
    (pps, way, spans)
}

/// `s` in at most two lines `w` wide, as a Library card's words are set: the
/// second ending in an ellipsis when more would follow.
fn two_lines(s: &str, w: f32) -> (String, String) {
    let lines = crate::text::wrap(400, 12.5, s, w);
    match lines.len() {
        0 => (String::new(), String::new()),
        1 => (lines[0].clone(), String::new()),
        2 => (lines[0].clone(), lines[1].clone()),
        _ => {
            let mut cut = lines[1..].join(" ");
            while !cut.is_empty() && crate::text::width(400, 12.5, &format!("{}…", cut.trim_end())) > w {
                cut.pop();
            }
            (lines[0].clone(), format!("{}…", cut.trim_end()))
        }
    }
}

/// Where a screen carried over the strip at `x` lands among the turns shown
/// (`spans`, left edges and widths, without its room): before the first whose
/// middle is right of the pointer. (Measured on the strip as shown, room and
/// all, so the room stays put under a pointer that stays put.)
fn slot_at(spans: &[(f64, f64)], x: f64) -> usize {
    spans.iter().position(|(l, w)| x < l + w / 2.0).unwrap_or(spans.len())
}

/// Bring the strip's blocks to `rows`: the same turns at the start and the
/// end keep their blocks (their places updated, so they glide), and only
/// those between are taken out and put in. A block is its turn's by its key.
fn sync_blocks(model: &VecModel<crate::PlBlock>, rows: Vec<crate::PlBlock>) {
    let old: Vec<crate::PlBlock> = (0..model.row_count()).filter_map(|i| model.row_data(i)).collect();
    let (n, m) = (old.len(), rows.len());
    let mut head = 0;
    while head < n && head < m && old[head].key == rows[head].key {
        head += 1;
    }
    let mut tail = 0;
    while tail < n - head && tail < m - head && old[n - 1 - tail].key == rows[m - 1 - tail].key {
        tail += 1;
    }
    for i in (head..n - tail).rev() {
        model.remove(i);
    }
    for (k, b) in rows[head..m - tail].iter().enumerate() {
        model.insert(head + k, b.clone());
    }
    for i in (0..head).chain(m - tail..m) {
        if model.row_data(i).as_ref() != Some(&rows[i]) {
            model.set_row_data(i, rows[i].clone());
        }
    }
}


/// The colours used lately, newest first, as the picker lists them.
pub(crate) fn recent_colors() -> Vec<[u8; 3]> {
    crate::uistate::get("recent_colours").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub(crate) fn keep_recent(c: [u8; 3]) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_fits_each_turn_to_its_time() {
        // the approved mock's Default: seven turns, 51 s in all, across 888 px
        let secs = [8.0, 6.0, 8.0, 5.0, 8.0, 6.0, 10.0];
        let (pps, way, spans) = lay_strip(&secs, None, 888.0);
        assert_eq!(way, 26.0);
        assert!((pps - (888.0 - 33.0 * 7.0) / 51.0).abs() < 1e-9);
        assert_eq!(spans[0].0, 27.0, "the first screen's bar after its way bar and the hairline");
        assert!((spans[0].1 - 103.06).abs() < 0.01);
        let end = spans[6].0 + spans[6].1;
        assert!((end - 882.0).abs() < 1e-9, "the last ends a turn's gap short of the strip's end: {end}");
        // twice the time, twice as long
        let (_, _, spans) = lay_strip(&[4.0, 8.0], None, 888.0);
        assert!((spans[1].1 - 2.0 * spans[0].1).abs() < 1e-9);
        // many turns: shorter way bars, never under 18
        let (_, short, _) = lay_strip(&[8.0; 30], None, 888.0);
        assert_eq!(short, 18.0);
        // an end dragged keeps the scale and the way bars while the strip has room
        let (_, kept, spans) = lay_strip(&[8.0, 20.0, 8.0], Some((pps, way)), 888.0);
        assert_eq!(kept, 26.0);
        assert!((spans[1].1 - 20.0 * pps).abs() < 1e-9);
        // and past the strip's room, it is fitted again: it never runs out of its box
        let (narrower, _, spans) = lay_strip(&[8.0, 60.0, 8.0], Some((pps, way)), 888.0);
        assert!(narrower < pps);
        let end = spans[2].0 + spans[2].1;
        assert!((end - 882.0).abs() < 1e-9, "ends where a fitted strip ends: {end}");
    }

    #[test]
    fn a_card_takes_two_lines_at_most() {
        assert_eq!(two_lines("The time and the date.", 204.0), ("The time and the date.".to_string(), String::new()));
        let (a, b) = two_lines("Says “A LONG LINE OF WORDS”, with the CPU temperature, the GPU load, the memory used and the time.", 204.0);
        assert!(!a.is_empty() && b.ends_with('…'), "{a} / {b}");
        assert!(crate::text::width(400, 12.5, &b) <= 204.0);
    }

    #[test]
    fn a_carried_screen_lands_where_the_pointer_is() {
        let (_, _, spans) = lay_strip(&[8.0, 8.0, 8.0], None, 888.0);
        let mid = |i: usize| spans[i].0 + spans[i].1 / 2.0;
        assert_eq!(slot_at(&spans, 2.0), 0, "before the first turn's middle: first");
        assert_eq!(slot_at(&spans, mid(0) + 1.0), 1);
        assert_eq!(slot_at(&spans, mid(1) - 1.0), 1);
        assert_eq!(slot_at(&spans, mid(2) + 1.0), 3, "past the last turn's middle: last");
        assert_eq!(slot_at(&[], 400.0), 0);
    }
}
