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
use crate::z10::{self, Entry, Page, View};
use ectodeck_protocol::Action;
use crate::{AppWindow, Cell, CheckItem, DeskItem, DeskPad, EffectItem, KeyItem, LayerItem, LibGroup, LibRow, MenuRow, MousePatch, ProfileItem};

/// The mouse's picture as the page shows it: its layers (for what is where
/// on it), and each piece as the page takes it.
struct MouseImages {
    layers: crate::mouse_art::Layers,
    base: slint::Image,
    lights: Vec<MousePatch>,
    chosen: Vec<MousePatch>,
    shapes: Vec<MousePatch>,
}

/// The part of the mouse's whole picture its page's hole shows, in pixels:
/// all of it the stage, which clips it, shows.
fn mouse_part(ui: &AppWindow, s: f32, l: &crate::mouse_art::Layers) -> (usize, usize, usize, usize) {
    let (bw, bh) = (l.base.width() as f32, l.base.height() as f32);
    let (px, py) = ((ui.get_mouse_pic_wx() * s).round(), (ui.get_mouse_pic_wy() * s).round());
    let (sx, sy) = ((ui.get_mouse_stage_wx() * s).round(), (ui.get_mouse_stage_wy() * s).round());
    let (sw, sh) = ((ui.get_mouse_stage_width() * s).round(), (ui.get_mouse_stage_height() * s).round());
    let (x0, y0) = (px.max(sx), py.max(sy));
    let (x1, y1) = ((px + bw).min(sx + sw), (py + bh).min(sy + sh));
    if x1 <= x0 || y1 <= y0 {
        return (0, 0, 0, 0);
    }
    ((x0 - px) as usize, (y0 - py) as usize, (x1 - x0) as usize, (y1 - y0) as usize)
}

/// The mouse's layers as the page takes them, drawn at `scale`.
fn mouse_images(l: crate::mouse_art::Layers, scale: f32) -> MouseImages {
    let image = |px: &resvg::tiny_skia::Pixmap| {
        let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(px.width(), px.height());
        buf.make_mut_bytes().copy_from_slice(px.data());
        slint::Image::from_rgba8_premultiplied(buf)
    };
    let patch = |p: &crate::mouse_art::Patch| MousePatch { img: image(&p.px), x: p.x as f32 / scale, y: p.y as f32 / scale, w: p.px.width() as f32 / scale, h: p.px.height() as f32 / scale };
    MouseImages {
        base: image(&l.base),
        lights: l.lights.iter().map(patch).collect(),
        chosen: l.chosen.iter().map(patch).collect(),
        shapes: l.shapes.iter().map(patch).collect(),
        layers: l,
    }
}

/// What a row of the New screen layout menu makes: a blank layout, a copy of the layout
/// with this id, or the layout Ectodeck ships with this key, as it shipped.
#[derive(Clone, Debug, PartialEq)]
pub enum NewFrom {
    Scratch,
    Copy(String),
    Ready(String),
}

/// The New screen layout menu, in either window: From scratch, then under a heading every
/// layout there is, to start from a copy of it, then under another each layout Ectodeck
/// ships that was deleted, to put back as it shipped; and what each row makes (None for the
/// rules and the headings).
pub fn new_rows(library: &[crate::lcd_page::Page]) -> (Vec<MenuRow>, Vec<Option<NewFrom>>) {
    let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into() };
    let mut rows = vec![row(0, "From scratch"), row(2, ""), row(1, "Duplicate a layout")];
    let mut made = vec![Some(NewFrom::Scratch), None, None];
    for p in library {
        rows.push(row(0, &p.name));
        made.push(Some(NewFrom::Copy(p.id.clone())));
    }
    let gone: Vec<crate::lcd_page::Page> = crate::lcd_page::missing_built_ins(library).into_iter().filter_map(crate::lcd_page::Page::built_in).collect();
    if !gone.is_empty() {
        rows.extend([row(2, ""), row(1, "Add a ready-made layout")]);
        made.extend([None, None]);
        for p in gone {
            rows.push(row(0, &p.name));
            made.push(Some(NewFrom::Ready(p.id)));
        }
    }
    (rows, made)
}

/// What the page asked for, done once its event has been handled.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// the mouse's page: a tab, a button or light from its list or its picture,
    /// a kind of job (and where its switch is), the job's menu opened, the
    /// button back to its factory job, an effect; the pointer on the picture
    /// (where, and whether over it) and a click there, the pointer on a
    /// button's words, and the next frame of the lights' effects
    MouseTab(i32),
    MouseRow(i32),
    MouseKind(i32, f32, f32, f32),
    MouseJob(f32, f32, f32),
    MouseReset,
    MouseResetAll,
    MouseEffect(i32),
    MousePicMoved(f32, f32, bool),
    MousePicClicked(f32, f32),
    MouseCallHover(i32, bool),
    MouseFrame,
    /// the mouse's profiles: one chosen, the one in use renamed, a new one,
    /// the menu on the one in use, and Undo after one was deleted
    MouseProfile(i32),
    MouseProfileRename(String),
    MouseProfileNew,
    MouseProfileMenu(f32, f32, f32),
    /// the menu on the keyboard's profile being edited
    KbProfileMenu(f32, f32, f32),
    MouseUndo,
    /// the Macro Editor, from the mouse's bar: on Ectodeck's own macros
    MouseMacroEditor,
    /// Home's desk: a press on a device (or, -1, the empty desk), the pointer
    /// moved pressed, let go, the wheel, the pointer over a device or off it,
    /// Arrange, Done, a zoom control (-1 out, 0 to 100%, 1 in, 2 the fit), and
    /// the next frame of its easing
    DeskPress(i32, f32, f32),
    DeskPan(f32, f32),
    DeskMove(f32, f32),
    DeskRelease,
    DeskWheel(f32, f32, f32),
    DeskHover(i32, bool),
    DeskArrange,
    DeskDone,
    DeskZoom(i32),
    DeskTick,
    /// a pad on the desk: the pointer over one or off it; a press on the
    /// chosen one's edges (1 left, 2 top, 4 right, 8 bottom, together at a
    /// corner) and where; its Color, where that is; a picture for it, from
    /// the desktop's file picker; its picture taken off; the pad taken off
    DeskPadHover(i32, bool),
    DeskPadEdges(i32, f32, f32),
    DeskPadColor(f32, f32, f32),
    DeskPadPicture,
    DeskPadNoPicture,
    DeskPadDelete,
    /// the desk's light: its menu, on Home; the next of its frames
    DeskLightMenu(f32, f32, f32),
    DeskLightFrame,
    /// the rebindings shown on the keyboard, or not
    ShowRebindings,
    Move,
    Minimize,
    Maximize,
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
    /// time to send the preview again: the effect every two seconds, streamed colors 14 times a second
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
    CarryMove(f32, bool, i32),
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

/// A picture asked for a pad on the desk: the desktop's file picker open for
/// it, then the file chosen being read (and kept in Ectodeck's data).
enum PadAsk {
    Picking(mpsc::Receiver<Option<std::path::PathBuf>>),
    Reading(mpsc::Receiver<Result<(std::path::PathBuf, crate::picture::Photo), String>>),
}

/// The color picker, open on a swatch: what it sets, the color it opened
/// on, and the color now, as hue (0 to 360), saturation and brightness (0 to 1).
struct Pick {
    ctl: Ctl,
    /// for a setting a core described (Ctl::Described): which
    piece: Option<crate::described::Piece>,
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

/// A rounded box's picture: `f` gives each pixel's color from its place (0 to 1 across and down).
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
    Minimize,
    Maximize,
    Close,
    /// open the Macro Editor's window, on a library and on this macro (0: as it was)
    OpenMacros(crate::macros::Lib, u32),
    /// how one of Ectodeck's macros plays, set in the Macro Editor, for the
    /// controls that run it
    MacroPlays(u32, ectodeck_protocol::Plays),
    /// one of Ectodeck's macros deleted: the controls that ran it do nothing now
    MacroGone(u32),
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
    let l = &page.draft()["lighting"]["keys"];
    let e = &l["effects"][l["effect"].as_str().unwrap_or("off")];
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

const LAYERS: [(&str, &str); 2] = [("base", "When pressed"), ("fn", "When Fn is held")];

/// The action menu's pages under "Other": each one's title and its list.
const PAGES: [&str; 7] = ["A key", "A media key", "A mouse button", "Switch profile", "Switch screen layout", "Switch playlist", "Keyboard function"];
/// the pages of profiles and of the screen's jobs among them, and the page of macros,
/// opened from the key's Macro switch (other devices' profiles, and Ectodeck's own
/// macros, are on those pages too: see jobs.rs)
const PROFILE_PAGE: usize = 3;
const LAYOUT_PAGE: usize = 4;
const PLAYLIST_PAGE: usize = 5;
const MACRO_PAGE: usize = 7;

/// The page of the action menu a screen job is on.
fn screen_page(code: u32) -> usize {
    if code >= z10::NEXT_PLAYLIST { PLAYLIST_PAGE } else { LAYOUT_PAGE }
}

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

#[derive(Clone)]
enum Choice {
    Nothing,
    Set(Action),
    Page(usize),
    /// the job that plays this playlist, by its id (its code found when it is given)
    Play(String),
    /// one of Ectodeck's own jobs, by its id (see jobs.rs; its code found when it is given)
    Job(String),
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
    /// the playlist each code a key may play one with plays (see z10::PLAY_CODES), by its
    /// id, as Ectodeck's settings keep it
    keyed: std::collections::BTreeMap<u32, String>,
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
    /// the Lighting page: the color Per key paints with, whether the keyboard
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
    /// the mouse's page
    pub mouse: crate::mouse::Page,
    /// what the system says of the mouse, looked at every two seconds, as the keyboard's
    mouse_status: Option<(Instant, serde_json::Value)>,
    mouse_icons: Option<(f32, Vec<slint::Image>)>,
    /// the mouse's picture in layers, for a scale, and the scale being drawn
    mouse_layers: Option<(f32, MouseImages)>,
    mouse_drawing: Option<f32>,
    /// the button under the pointer, on the picture or on its words
    mouse_hover: Option<u8>,
    /// the chosen and hovered buttons the picture shows now
    mouse_shows: (Option<u8>, Option<u8>),
    /// the lights' effects played on the picture, 30 frames a second, while they move
    mouse_frames: Option<slint::Timer>,
    /// the desk's light (see desk_light.rs): its effect, from when, its
    /// frames' timer, when each device was last sent its part (the mouse's
    /// with the colors it was sent), the colors it gives now (each key by its
    /// place, the mouse's logo and wheel), and whether its menu is open
    desk_light: crate::desk_light::Effect,
    desk_light_t0: Instant,
    desk_light_timer: Option<slint::Timer>,
    desk_light_mouse: (Option<Instant>, [[u8; 3]; 2]),
    desk_colors: (HashMap<u32, [u8; 3]>, [Option<[u8; 3]>; 2]),
    desk_light_menu: bool,
    /// the mouse in HDR on its page: its lamps and unlit picture, for the
    /// layers' scale; the picture lit as it shows now, and the part of the
    /// whole picture the page's hole shows of it (in pixels)
    mouse_hdr: Option<(f32, crate::mouse_art::Hdr)>,
    mouse_lit: Option<Rc<crate::picture::Lit>>,
    mouse_part: (usize, usize, usize, usize),
    /// the light the mouse is lit by while the Key lab tunes it (none: as its
    /// core's file has it), and the drawing's pixels to a millimeter
    mouse_light: Option<serde_json::Value>,
    mouse_px_mm: f32,
    /// the mouse in HDR on Home's desk: its lamps for the width its layers
    /// there were drawn at, and its picture lit as it shows now
    desk_mouse_hdr: Option<(u32, crate::mouse_art::Hdr)>,
    desk_mouse_lit: Option<Rc<crate::picture::Lit>>,
    /// Home's desk, its easing's frames while it eases, and the mouse's picture
    /// drawn for its size on the desk (its width in pixels), and the size being drawn
    pub desk: crate::desk::Desk,
    desk_frames: Option<slint::Timer>,
    desk_mouse: Option<(u32, MouseImages)>,
    desk_mouse_drawing: Option<u32>,
    /// the pads' pictures as read, by their files; those being read, and
    /// those that couldn't be (each pad shows its color for those); the
    /// picture asked for a pad (the one at that place in the pads, in that
    /// box then); what the desk says of one that couldn't be used
    pad_photos: HashMap<String, std::sync::Arc<crate::picture::Photo>>,
    pad_reading: Vec<(String, mpsc::Receiver<Result<crate::picture::Photo, String>>)>,
    pad_unread: std::collections::HashSet<String>,
    pad_asked: Option<(usize, [f32; 4], PadAsk)>,
    pad_note: String,
    /// whether the press now shut the color picker (a press that shuts it
    /// does nothing else on the desk)
    press_shut: bool,
    pick: Option<Pick>,
    overlay_model: Rc<VecModel<crate::Overlay>>,
    /// what each slider controls, which slider's menu is open, and what the
    /// sliders last read and last set
    bind: [String; 2],
    bind_menu: Option<(usize, (f32, f32, f32))>,
    /// the menu on the mouse's profile in use is open
    profile_menu: bool,
    /// the menu on the keyboard's profile, and the menu of apps a profile is
    /// tied to (which device, 0 the keyboard and 1 the mouse; which profile:
    /// the keyboard's by its number, the mouse's by its place; where it hangs)
    kb_profile_menu: bool,
    apps_menu: Option<(usize, usize, (f32, f32, f32))>,
    /// the apps seen in front (see front_app.rs), and for each device the
    /// profile chosen by hand, which it goes back to once an app it followed
    /// leaves the front (0 the keyboard's by its number, 1 the mouse's by its place)
    front_apps: crate::front_app::Seen,
    follow_back: [Option<usize>; 2],
    /// Ectodeck's macros played, for the controls given one (made when first needed)
    typist: Option<crate::typist::Player>,
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
    /// the Sensors box in Settings: the tab open, the folded runs opened (by device and
    /// run), the sensor being renamed and what is typed, what its pieces stood for as last
    /// drawn; the computer's sensors and their readings, read once a second while it shows;
    /// and the sensor asked about before it is unchecked
    sensor_tab: crate::sensor_list::Part,
    sensor_open: std::collections::BTreeSet<(String, crate::sensor_list::Run)>,
    renaming: Option<(String, String)>,
    sensor_map: crate::ecto_settings::SensorMap,
    sensors_seen: Option<(Instant, crate::sensor_list::Catalog, std::collections::HashMap<String, crate::sensor_list::Val>)>,
    unchecking: Option<String>,
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

/// A profile's color sample: how many colors, and the colors.
fn swatch(cols: &[[f64; 3]]) -> (i32, [Color; 4]) {
    let c = |i: usize| cols.get(i).or(cols.last()).map_or(Color::from_rgb_u8(0, 0, 0), |v| Color::from_rgb_u8(v[0] as u8, v[1] as u8, v[2] as u8));
    (cols.len().clamp(1, 4) as i32, [c(0), c(1), c(2), c(3)])
}

impl Front {
    pub fn new(ui: AppWindow, kb: Option<mpsc::Sender<device::Ask>>, controlled: bool, settings: crate::settings::Settings) -> Front {
        // (which sensors the screen's layouts can show, as last chosen; the Sensors card's
        // choices before, carried over once the layouts are read)
        let (setup, before) = crate::sensors::Setup::of(&settings.all()["sensors"]);
        crate::sensors::set_up(setup);
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
        on!(on_mouse_tab_chosen, |i| Ask::MouseTab(i));
        on!(on_mouse_row, |i| Ask::MouseRow(i));
        on!(on_mouse_kind_picked, |i, x, y, h| Ask::MouseKind(i, x, y, h));
        on!(on_mouse_job_clicked, |x, y, h| Ask::MouseJob(x, y, h));
        on!(on_mouse_reset, Ask::MouseReset);
        on!(on_mouse_reset_all, Ask::MouseResetAll);
        on!(on_mouse_pic_moved, |x, y, over| Ask::MousePicMoved(x, y, over));
        on!(on_mouse_pic_clicked, |x, y| Ask::MousePicClicked(x, y));
        on!(on_mouse_call_hover, |i, on| Ask::MouseCallHover(i, on));
        on!(on_mouse_profile_choose, |i| Ask::MouseProfile(i));
        on!(on_mouse_profile_rename, |t| Ask::MouseProfileRename(t.to_string()));
        on!(on_mouse_profile_new, Ask::MouseProfileNew);
        on!(on_mouse_profile_menu, |x, y, h| Ask::MouseProfileMenu(x, y, h));
        on!(on_kb_profile_menu, |x, y, h| Ask::KbProfileMenu(x, y, h));
        on!(on_mouse_undo, Ask::MouseUndo);
        on!(on_mouse_macro_editor, Ask::MouseMacroEditor);
        on!(on_desk_press, |i, x, y| Ask::DeskPress(i, x, y));
        on!(on_desk_pan, |x, y| Ask::DeskPan(x, y));
        on!(on_desk_move, |x, y| Ask::DeskMove(x, y));
        on!(on_desk_release, Ask::DeskRelease);
        on!(on_desk_wheel, |x, y, dy| Ask::DeskWheel(x, y, dy));
        on!(on_desk_hover, |i, on| Ask::DeskHover(i, on));
        on!(on_desk_arrange, Ask::DeskArrange);
        on!(on_desk_done, Ask::DeskDone);
        on!(on_desk_zoom_step, |how| Ask::DeskZoom(how));
        on!(on_desk_pad_hover, |i, on| Ask::DeskPadHover(i, on));
        on!(on_desk_pad_edges, |e, x, y| Ask::DeskPadEdges(e, x, y));
        on!(on_desk_pad_color, |x, y, h| Ask::DeskPadColor(x, y, h));
        on!(on_desk_pad_picture, Ask::DeskPadPicture);
        on!(on_desk_pad_no_picture, Ask::DeskPadNoPicture);
        on!(on_desk_pad_delete, Ask::DeskPadDelete);
        on!(on_desk_light_menu, |x, y, h| Ask::DeskLightMenu(x, y, h));
        on!(on_mouse_effect, |i| Ask::MouseEffect(i));
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
        on!(on_pl_carry_move, |x, near, shelf| Ask::CarryMove(x, near, shelf));
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
            mouse: crate::mouse::Page::new(),
            mouse_status: None,
            mouse_icons: None,
            mouse_layers: None,
            mouse_drawing: None,
            mouse_hover: None,
            mouse_shows: (None, None),
            mouse_frames: None,
            desk_light: crate::desk_light::Effect::Off,
            desk_light_t0: Instant::now(),
            desk_light_timer: None,
            desk_light_mouse: (None, [[0; 3]; 2]),
            desk_colors: Default::default(),
            desk_light_menu: false,
            mouse_hdr: None,
            mouse_lit: None,
            mouse_part: (0, 0, 0, 0),
            mouse_light: None,
            mouse_px_mm: 1.0,
            desk_mouse_hdr: None,
            desk_mouse_lit: None,
            desk: crate::desk::Desk::new(),
            desk_frames: None,
            desk_mouse: None,
            desk_mouse_drawing: None,
            pad_photos: HashMap::new(),
            pad_reading: Vec::new(),
            pad_unread: Default::default(),
            pad_asked: None,
            pad_note: String::new(),
            press_shut: false,
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
            keyed: Default::default(),
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
            profile_menu: false,
            kb_profile_menu: false,
            apps_menu: None,
            front_apps: crate::front_app::Seen::load(),
            follow_back: [None, None],
            typist: None,
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
            sensor_tab: crate::sensor_list::Part::Processor,
            sensor_open: Default::default(),
            renaming: None,
            sensor_map: Default::default(),
            sensors_seen: None,
            unchecking: None,
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
        f.give_built_ins();
        if let Some(b) = before {
            f.carry_sensors(&b);
        }
        f.keyed = f
            .settings
            .get("devices.z10.screen.keyed")
            .and_then(|v| v.as_object().cloned())
            .map(|m| m.iter().filter_map(|(c, id)| Some((c.parse::<u32>().ok().filter(|c| z10::PLAY_CODES.contains(c))?, id.as_str()?.to_string()))).collect())
            .unwrap_or_default();
        f.screens_noted = f.screens.clone();
        f.screens_saved = f.screens.clone();
        f.weather.set_place(weather_place(&f.settings));
        // the desk's light, as it was left playing
        let light = f.settings.get("desk.light").and_then(|v| v.as_str().map(crate::desk_light::Effect::of)).unwrap_or(crate::desk_light::Effect::Off);
        if light != crate::desk_light::Effect::Off && controlled {
            f.set_desk_light(light);
        }
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
                        let was_custom = sent.as_ref().is_some_and(|d| d["lighting"]["keys"]["effect"] == "custom");
                        self.page.saved_profile(n, p, unchanged);
                        if unchanged && was_custom {
                            if let Some(d) = self.page.drafts.get_mut(&n) {
                                d["lighting"]["keys"]["effect"] = serde_json::json!("custom");
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
        let picking = (self.picking.is_some() || self.pad_asked.is_some() || !self.pad_reading.is_empty()).then(|| Duration::from_millis(100));
        // (and when Home's desk will have settled, for its keyboard to be drawn for its size)
        let desk = (self.ui.get_place() == "home" && !self.desk_settled()).then(|| Duration::from_millis(150).saturating_sub(self.desk.changed.elapsed()) + Duration::from_millis(10));
        [self.failed_at.map(|t| Duration::from_secs(3).saturating_sub(t.elapsed()) + Duration::from_millis(10)), picking, desk].into_iter().flatten().min()
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
        self.poll_pads();
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
                Ask::Minimize => window.push(WindowAsk::Minimize),
                Ask::Maximize => window.push(WindowAsk::Maximize),
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
                        d["lighting"]["keys"]["effect"] = serde_json::json!(mode);
                    }
                    self.push_preview();
                }
                Ask::MouseTab(i) => {
                    self.close_menu();
                    self.close_picker();
                    self.mouse.tab = i.clamp(0, 3) as usize;
                }
                Ask::MouseRow(i) => {
                    self.close_menu();
                    self.close_picker();
                    if self.mouse.tab == 1 {
                        self.mouse.light = (i.max(0) as usize).min(1);
                    } else {
                        // selecting the chosen button again lets it go
                        let b = (i.max(0) as usize).min(7);
                        self.mouse.button = (self.mouse.button != Some(b)).then_some(b);
                    }
                }
                Ask::MouseKind(i, x, y, h) => {
                    let k = i.max(0) as usize;
                    let choices = self.mouse.choices_for(k);
                    if choices.len() == 1 {
                        // a kind with one job: that job, at once
                        self.close_menu();
                        if let Some(b) = self.mouse.button {
                            self.mouse.set_job(b, choices[0].1.clone());
                        }
                    } else {
                        self.open_mouse_menu(k, (x, y, h));
                    }
                }
                Ask::MouseJob(x, y, h) => {
                    if let Some(b) = self.mouse.button {
                        let k = crate::mouse::kind_of(&self.mouse.job(b));
                        self.open_mouse_menu(k, (x, y, h));
                    }
                }
                Ask::MouseReset => {
                    if let Some(b) = self.mouse.button {
                        self.mouse.reset_job(b);
                    }
                }
                Ask::MouseResetAll => {
                    for b in 0..crate::mouse::BUTTONS.len() {
                        if self.mouse.rebound(b) {
                            self.mouse.reset_job(b);
                        }
                    }
                }
                Ask::MouseProfile(i) => {
                    self.close_menu();
                    self.ui.set_mouse_renaming(false);
                    self.mouse.choose_profile(i.max(0) as usize);
                    // (chosen by hand: the one to come back to)
                    self.follow_back[1] = None;
                    // (what the mouse plays, new, on its picture)
                    self.show_mouse_lights();
                }
                Ask::MouseProfileRename(t) => self.mouse.rename_profile(&t),
                Ask::MouseProfileNew => {
                    self.close_menu();
                    self.mouse.new_profile();
                    self.show_mouse_lights();
                    // (its name opens to change)
                    self.ui.set_mouse_renaming(true);
                }
                Ask::MouseProfileMenu(x, y, h) => self.open_profile_menu((x, y, h)),
                Ask::MouseUndo => {
                    self.mouse.undo();
                    self.show_mouse_lights();
                }
                Ask::MouseMacroEditor => {
                    self.close_menu();
                    // (on the chosen button's macro, when it runs one)
                    let n = match self.mouse.button.and_then(|i| Action::read(&self.mouse.job(i))) {
                        Some(Action::Macro { n, .. }) => n,
                        _ => 0,
                    };
                    window.push(WindowAsk::OpenMacros(crate::macros::Lib::Ectodeck, n));
                }
                Ask::MouseEffect(i) => {
                    if let Some(e) = self.mouse.effects().get(i.max(0) as usize) {
                        let old = self.mouse.light_of(self.mouse.light_name());
                        self.mouse.set_light(crate::described::with_effect(&old, e));
                    }
                }
                Ask::MousePicMoved(x, y, over) => {
                    changed = false;
                    let (button, light) = if over { self.mouse_pic_at(x, y) } else { (None, None) };
                    self.ui.set_mouse_pic_hand(button.is_some() || light.is_some());
                    if button != self.mouse_hover {
                        self.mouse_hover = button;
                        self.show_mouse_picked();
                    }
                }
                Ask::MousePicClicked(x, y) => match self.mouse_pic_at(x, y) {
                    (Some(b), _) => {
                        self.close_menu();
                        self.close_picker();
                        let b = b as usize - 1;
                        self.mouse.button = (self.mouse.button != Some(b)).then_some(b);
                    }
                    (_, Some(l)) => {
                        self.close_menu();
                        self.close_picker();
                        self.mouse.light = l;
                    }
                    _ => changed = false,
                },
                Ask::MouseCallHover(i, on) => {
                    changed = false;
                    let b = Some(i.clamp(0, 7) as u8 + 1);
                    if on {
                        self.mouse_hover = b;
                    } else if self.mouse_hover == b {
                        self.mouse_hover = None;
                    }
                    self.show_mouse_picked();
                }
                Ask::MouseFrame => {
                    changed = false;
                    if self.mouse_pictured() {
                        self.show_mouse_lights();
                    } else {
                        self.mouse_frames = None;
                    }
                }
                Ask::DeskPress(i, x, y) => {
                    changed = false;
                    // (a press that shut the color picker does nothing else)
                    if !(self.press_shut && i < 0) {
                        self.desk.press(usize::try_from(i).ok(), (x, y));
                    }
                    self.show_desk();
                }
                Ask::DeskPan(x, y) => {
                    changed = false;
                    self.desk.pan_from((x, y));
                    self.show_desk();
                }
                Ask::DeskMove(x, y) => {
                    changed = false;
                    if self.desk.moved((x, y)) {
                        self.show_desk();
                    }
                }
                Ask::DeskRelease => {
                    changed = false;
                    if let Some(place) = self.desk.release().and_then(|i| self.desk.items.get(i)).map(|it| it.place.clone()) {
                        self.close_menu();
                        self.ui.set_place(place.into());
                        changed = true;
                    }
                    self.show_desk();
                }
                Ask::DeskWheel(x, y, dy) => {
                    changed = false;
                    if self.desk.wheel((x, y), dy) {
                        self.show_desk();
                    }
                }
                Ask::DeskHover(i, on) => {
                    changed = false;
                    let i = usize::try_from(i).ok();
                    if on {
                        self.desk.hover = i;
                    } else if self.desk.hover == i {
                        self.desk.hover = None;
                    }
                    self.show_desk();
                }
                Ask::DeskArrange => {
                    changed = false;
                    self.desk.set_editing(true);
                    self.desk_frames_run();
                    self.show_desk();
                }
                Ask::DeskDone => {
                    changed = false;
                    self.desk.set_editing(false);
                    self.desk_frames_run();
                    self.show_desk();
                }
                Ask::DeskZoom(how) => {
                    changed = false;
                    self.desk.zoom_step(how);
                    self.desk_frames_run();
                }
                Ask::DeskLightMenu(x, y, h) => self.open_desk_light_menu((x, y, h)),
                Ask::DeskLightFrame => {
                    changed = false;
                    self.desk_light_frame();
                }
                Ask::DeskTick => {
                    changed = false;
                    self.desk.tick(Instant::now());
                    self.show_desk();
                    self.desk_frames_run();
                }
                Ask::DeskPadHover(i, on) => {
                    changed = false;
                    let i = usize::try_from(i).ok();
                    if on {
                        self.desk.pad_hover = i;
                    } else if self.desk.pad_hover == i {
                        self.desk.pad_hover = None;
                    }
                    self.show_desk();
                }
                Ask::DeskPadEdges(e, x, y) => {
                    changed = false;
                    self.desk.press_edges([e & 1 != 0, e & 2 != 0, e & 4 != 0, e & 8 != 0], (x, y));
                    self.show_desk();
                }
                Ask::DeskPadColor(x, y, h) => {
                    changed = false;
                    if self.desk.pad_chosen.is_some() {
                        self.open_picker(Ctl::DeskPad, None, 0, (x, y, h));
                    }
                }
                Ask::DeskPadPicture => {
                    changed = false;
                    self.ask_pad_picture();
                }
                Ask::DeskPadNoPicture => {
                    changed = false;
                    if let Some((i, b)) = self.desk.chosen_box() {
                        self.desk.set_pad_picture(i, b, None);
                        self.pad_note.clear();
                        self.show_desk();
                    }
                }
                Ask::DeskPadDelete => {
                    changed = false;
                    self.close_picker();
                    self.desk.delete_pad();
                    self.pad_note.clear();
                    self.show_desk();
                }
                Ask::CellPick(id, i, x, y, h) => match crate::described::piece_of(id) {
                    Some(piece) => self.described_pick(piece, i.max(0) as usize, (x, y, h)),
                    None => self.cell_pick(lighting::ctl(id), i.max(0) as usize, (x, y, h)),
                },
                // (a setting a core described: the value its slider stands for)
                Ask::CellSlide(id, v) if crate::described::piece_of(id).is_some() => {
                    let piece = crate::described::piece_of(id).unwrap();
                    if let Some((sid, d, _)) = self.mouse.described(piece) {
                        self.mouse.set_described(piece, crate::described::slid(&sid, &d, v));
                    }
                }
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
                        self.screens.turns(self.page.editing).get(i).map(|e| Carry { from_strip: true, index: i, page: e.page.clone(), secs: e.secs, slot: Some(i), home: Some(i), shelf: None })
                    } else {
                        self.screens.library.get(i).map(|p| Carry { from_strip: false, index: i, page: p.id.clone(), secs: crate::lcd_page::SECS.2, slot: None, home: None, shelf: None })
                    };
                    if let Some(p) = self.carry.as_ref().and_then(|c| self.screens.library.iter().find(|p| p.id == c.page)) {
                        self.ui.set_carry_name(p.name.as_str().into());
                    }
                }
                Ask::CarryMove(x, near, shelf) => {
                    changed = false;
                    let spans = self.strip_spans.clone();
                    // a layout carried along the Library: the place there it lands in, past the
                    // last card the last
                    let n = self.screens.library.len();
                    if let Some(c) = self.carry.as_mut().filter(|c| !c.from_strip) {
                        let at = usize::try_from(shelf).ok().filter(|_| n > 0).map(|s| s.min(n - 1));
                        if at != c.shelf {
                            c.shelf = at;
                            changed = true;
                        }
                    }
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
                        if let Some(to) = c.shelf {
                            if to != c.index {
                                self.move_layout(c.index, to);
                            }
                        } else if let Some(id) = self.ob_over.take() {
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
                    if let (Some(id), Some((_, cat, _))) = (self.unchecking.take(), self.sensors_seen.clone()) {
                        let mut s = crate::sensors::setup();
                        s.check(&cat, &id, false);
                        self.keep_setup(s);
                    } else if let Some(k) = self.deleting.take().and_then(|id| self.screens.index_of(&id)) {
                        self.delete_layout(k);
                    }
                    self.ui.set_confirm_open(false);
                }
                Ask::ConfirmNo => {
                    self.deleting = None;
                    self.unchecking = None;
                    self.ui.set_confirm_open(false);
                }
                Ask::LcdTick => {
                    // (on Home, the mouse's picture drawn for its size on the desk once it settles)
                    if self.ui.get_place() == "home" {
                        self.desk_mouse_want();
                    }
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
                    // the Sensors box's readings, once a second while Settings is open
                    if self.ui.get_place() == "settings" && self.sensors_seen.as_ref().is_some_and(|(t, _, _)| t.elapsed() >= Duration::from_secs(1)) {
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
                    // the parts of the switch, as the menu's pages are; Nothing at once (and
                    // Screen opens the page of the key's own screen job, if it has one)
                    match i {
                        7 => {
                            self.close_menu();
                            self.assign(self.layer, Action::None);
                        }
                        _ => {
                            let screen = self.page.sel.and_then(|pos| z10::screen_job(&self.page.def(LAYERS[self.layer].0, pos))).map_or(LAYOUT_PAGE, screen_page);
                            let page = [0, 1, 2, MACRO_PAGE, 3, screen, 6][i.clamp(0, 6) as usize];
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
                    // (the line keeps what's typed; the box isn't drawn again under it)
                    Ctl::SensorName => {
                        if let Some((_, typed)) = &mut self.renaming {
                            *typed = t;
                        }
                        changed = false;
                    }
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
                Ask::Profile(n) => {
                    self.choose_profile(n as u8);
                    self.follow_back[0] = None;
                }
                Ask::KbProfileMenu(x, y, h) => self.open_kb_profile_menu((x, y, h)),
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
                    let first = self.menu.as_ref().and_then(|m| m.choices.iter().position(|c| matches!(c, Choice::Set(_) | Choice::Page(_) | Choice::Play(_) | Choice::Job(_))));
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
                        (Some((layer, _)), Some(pos)) => match self.page.def(layer, pos) {
                            Action::Macro { n, .. } => n,
                            _ => 0,
                        },
                        _ => 0,
                    };
                    window.push(WindowAsk::OpenMacros(crate::macros::Lib::Keyboard, n));
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
        if self.macros.is_none() {
            return Vec::new();
        }
        // (where Ectodeck's macros run, as the mouse's profiles say now)
        self.macros_elsewhere();
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

    /// The display's text from the file another program's frames are written to: the
    /// keyboard will not read its display back, so the z10-lcd service writes each frame it
    /// sends there; if nothing has written it for ten seconds, the panel is drawn unlit rather
    /// than stale. (While Ectodeck's own player drives the screen, the picture follows each
    /// frame as the player sends it instead: see [Front::screen_news].)
    fn poll_lcd(&mut self) {
        if self.player_drives() {
            return;
        }
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let f = dir.join("z10-lcd/frame");
        let fresh = std::fs::metadata(&f).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a.as_secs() < 10);
        let rows: Vec<String> = std::fs::read_to_string(&f).map(|s| s.split('\n').map(String::from).collect()).unwrap_or_default();
        self.show_lcd(fresh, &rows);
    }

    /// Whether Ectodeck's own player is what the keyboard's screen shows now.
    fn player_drives(&self) -> bool {
        self.controlled && matches!(self.lcd.state, crate::lcd_player::State::Playing | crate::lcd_player::State::Paused)
    }

    /// The display in the picture of the keyboard showing `rows` (unlit unless `live`),
    /// in the screen's light, drawn again only when that changes.
    fn show_lcd(&mut self, live: bool, rows: &[String]) {
        let rows: Vec<String> = rows.iter().take(4).map(|r| format!("{:<20}", r.chars().take(20).collect::<String>())).collect();
        let lcd = (live && !rows.is_empty(), rows, self.page.lcd_color());
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
        // the screen's clock, so the keys show the moment of the effect it draws
        body["t"] = serde_json::json!(self.t0.elapsed().as_millis() as u64);
        body["tab"] = serde_json::json!("native");
        body["claim"] = serde_json::json!(true);
        self.preview_asked = true;
        self.ask_kb(device::Ask::Preview(body));
    }

    /// A piece of a setting a core described used: a color opens the color
    /// picker, a choice or a switch takes its new value.
    fn described_pick(&mut self, piece: crate::described::Piece, index: usize, anchor: (f32, f32, f32)) {
        let Some((_, d, v)) = self.mouse.described(piece) else { return };
        if crate::described::color_at(&d, &v, index).is_some() {
            self.open_picker(Ctl::Described, Some(piece), index, anchor);
            return;
        }
        if let Some(nv) = crate::described::picked(&d, &v, index) {
            self.mouse.set_described(piece, nv);
        }
        self.refresh();
    }

    /// A piece of the tile used: a swatch opens the color picker, the rest change the draft.
    fn cell_pick(&mut self, c: Ctl, index: usize, _anchor: (f32, f32, f32)) {
        match c {
            Ctl::MouseSameWheel => {
                self.mouse.same_wheel = index % 2 == 1;
                crate::uistate::set("mouse_same_wheel", serde_json::json!(self.mouse.same_wheel));
                if self.mouse.same_wheel {
                    self.mouse.copy_logo_to_wheel();
                }
                self.refresh();
                return;
            }
            Ctl::Color | Ctl::Colors | Ctl::Background | Ctl::Paint | Ctl::LcdColor => {
                self.open_picker(c, None, index, _anchor);
                return;
            }
            Ctl::Start | Ctl::Stop | Ctl::Submode | Ctl::Plays => {
                self.open_light_menu(c, _anchor);
                return;
            }
            Ctl::WhereMode | Ctl::WhereAll | Ctl::Zone => {
                let mode = self.page.draft()["lighting"]["keys"]["effect"].as_str().unwrap_or("off").to_string();
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
            Ctl::SensorTab => {
                if let Some(p) = self.sensor_map.tabs.get(index) {
                    self.sensor_tab = *p;
                    self.renaming = None;
                }
                return;
            }
            Ctl::Sensor => {
                if let Some(id) = self.sensor_map.chips.get(index).cloned() {
                    self.toggle_sensor(&id);
                }
                return;
            }
            Ctl::SensorPencil => {
                if let Some(id) = self.sensor_map.chips.get(index).cloned() {
                    let now = crate::sensors::setup().names.get(&id).cloned().or_else(|| self.sensors_seen.as_ref().and_then(|(_, cat, _)| cat.sensor(&id)).map(|(_, s)| s.name.clone()));
                    self.renaming = Some((id, now.unwrap_or_default()));
                }
                return;
            }
            Ctl::SensorRun => {
                if let Some(r) = self.sensor_map.runs.get(index).cloned() {
                    if !self.sensor_open.remove(&r) {
                        self.sensor_open.insert(r);
                    }
                }
                return;
            }
            Ctl::MakeMain => {
                if let (Some(key), Some((_, cat, _))) = (self.sensor_map.mains.get(index).cloned(), self.sensors_seen.clone()) {
                    let mut s = crate::sensors::setup();
                    s.make_main(&cat, &key);
                    self.keep_setup(s);
                }
                return;
            }
            // a new name given (Enter, or the line left), or not (Escape)
            Ctl::SensorName => {
                if let (Some((id, typed)), Some((_, cat, _))) = (self.renaming.take(), self.sensors_seen.clone()) {
                    if index == 1 {
                        let mut s = crate::sensors::setup();
                        s.rename(&cat, &id, &typed);
                        self.keep_setup(s);
                    }
                }
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

    // ------------------------------------------------------------ the color picker

    fn open_picker(&mut self, c: Ctl, piece: Option<crate::described::Piece>, index: usize, anchor: (f32, f32, f32)) {
        // the second click on the swatch whose picker a press just closed
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        self.close_menu();
        self.close_picker();
        let rgb = match piece.and_then(|p| self.mouse.described(p)) {
            Some((_, d, v)) => crate::described::color_at(&d, &v, index).unwrap_or([0xff, 0, 0]),
            None if c == Ctl::DeskPad => self.desk.pad_chosen.and_then(|i| self.desk.pads.get(i)).map_or(crate::desk::PAD_COLOR, |p| p.color),
            None => lighting::color_of(&self.page, c, index, self.paint),
        };
        self.pick = Some(Pick { ctl: c, piece, index, start: rgb, hsv: rgb2hsv(rgb), rgb, anchor });
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
            // (a pad's color, changed as the picker moved, kept as it closes)
            if p.ctl == Ctl::DeskPad && p.rgb != p.start {
                self.desk.save_pads();
            }
            // the color the picker is closed on joins the recent ones
            if p.rgb != p.start {
                keep_recent(p.rgb);
            }
        }
    }

    /// Show the color everywhere but the field being typed in.
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

    /// A new color in the picker: shown, and set where the picker was opened.
    fn pick_set(&mut self, hsv: [f32; 3], rgb: Option<[u8; 3]>, skip: Option<i32>) {
        let Some(p) = &mut self.pick else { return };
        p.hsv = hsv;
        p.rgb = rgb.unwrap_or_else(|| hsv2rgb(hsv));
        let (c, piece, index, rgb) = (p.ctl, p.piece, p.index, p.rgb);
        if c == Ctl::Paint {
            self.paint = rgb;
        } else if c == Ctl::DeskPad {
            self.desk.set_pad_color(rgb);
            self.show_desk();
        } else if let Some(piece) = piece {
            if let Some((_, d, v)) = self.mouse.described(piece) {
                self.mouse.set_described(piece, crate::described::with_color(&d, &v, index, rgb));
            }
            self.refresh();
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
            if self.page.draft()["lighting"]["keys"]["effect"] == "custom" {
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
                    self.assign(layer, z10::pressed_action(held, hid));
                }
            } else if hid == 0 {
                // a lone modifier pressed and let go assigns that modifier
                if held == mods {
                    self.assign(layer, z10::pressed_action(held, 0));
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

    fn assign(&mut self, layer: usize, a: Action) {
        self.listening = None;
        if let Some(pos) = self.page.sel {
            self.page.set_def(LAYERS[layer].0, pos, &a);
        }
        self.refresh();
    }

    // ------------------------------------------------------------ the action menu

    fn open_menu(&mut self, layer: usize, anchor: (f32, f32, f32)) {
        // the job's own kind of list, or the whole menu for a key that does nothing
        let page = self.page.sel.map(|pos| self.page.def(LAYERS[layer].0, pos)).and_then(|d| match z10::kind_of(&d) {
            "Key" => Some(0),
            "Media" => Some(1),
            "Mouse" => Some(2),
            "Macro" => Some(MACRO_PAGE),
            "Profile" => Some(3),
            "Screen" => z10::screen_job(&d).map(screen_page),
            "Keyboard" => Some(6),
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
        if self.bind_menu.take().is_some() || self.light_menu.take().is_some() || self.way_menu.take().is_some() || self.time_menu.take().is_some() || std::mem::take(&mut self.pl_menu) || std::mem::take(&mut self.add_menu) || std::mem::take(&mut self.ob_menu) || self.lib_menu.take().is_some() || self.new_menu.take().is_some() {
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
        if self.mouse.menu.take().is_some() || std::mem::take(&mut self.profile_menu) || std::mem::take(&mut self.kb_profile_menu) || self.apps_menu.take().is_some() || std::mem::take(&mut self.desk_light_menu) {
            self.ui.set_menu_open(false);
            self.ui.set_menu_w(300.0);
            self.ui.set_mouse_menu_open(false);
        }
        // (the next menu keeps a column for ticks, unless it has none)
        self.ui.set_menu_marks(true);
    }

    /// A press somewhere in the window, in its logical pixels: a press away
    /// from the open menu closes it, and a press on the button it hangs from
    /// keeps that button's click from opening it again.
    pub fn pressed_at(&mut self, x: f32, y: f32) {
        self.press_shut = false;
        self.let_chosen_go(x, y);
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
        if self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.time_menu.is_some() || self.pl_menu || self.add_menu || self.ob_menu || self.lib_menu.is_some() || self.new_menu.is_some() {
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
                if (self.light_menu.is_some() || self.way_menu.is_some() || self.time_menu.is_some() || self.pl_menu || self.add_menu || self.ob_menu) && x >= ax && x < ax + aw && y >= ay && y < ay + ah {
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
                // (a swatch is 34 wide; a pad's Color button, its swatch and its word)
                let aw = if self.pick.as_ref().is_some_and(|p| p.ctl == Ctl::DeskPad) { crate::text::width(500, 13.0, "Color") + 42.0 } else { 34.0 };
                if x >= ax && x < ax + aw && y >= ay && y < ay + ah {
                    self.shut = Some(((ax, ay), Instant::now()));
                }
                self.close_picker();
                self.press_shut = true;
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

    /// The rule for whatever a page has chosen (a key on the keyboard, a
    /// button on the mouse), the same on every page: selecting it again lets
    /// it go, Escape lets it go, and so does a press anywhere off it. A press
    /// keeps it chosen only on what chooses (where a press chooses another,
    /// or the same one again) and on its editor; a press while a menu or the
    /// color picker is open only closes that; one on the title bar only moves
    /// the window.
    fn let_chosen_go(&mut self, x: f32, y: f32) {
        // (a press while any menu is open is the menu's: it chooses there, or shuts it)
        if y < 36.0 || self.ui.get_menu_open() || self.menu.is_some() || self.mouse.menu.is_some() || self.profile_menu || self.kb_profile_menu || self.apps_menu.is_some() || self.desk_light_menu || self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.pick.is_some() || self.place_menu {
            return;
        }
        let Some(keeps) = self.chosen_keeps() else { return };
        if keeps.iter().any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h) {
            return;
        }
        // (a button on the mouse's picture chooses too)
        if self.ui.get_place() == "mouse" && self.mouse_pic_at_window(x, y).0.is_some() {
            return;
        }
        self.let_chosen_go_now();
        self.refresh();
    }

    /// Where a press keeps the page's chosen thing chosen, in the window's
    /// logical pixels; none when nothing is chosen.
    fn chosen_keeps(&self) -> Option<Vec<(f32, f32, f32, f32)>> {
        let ui = &self.ui;
        match ui.get_place().as_str() {
            // the keyboard (a key there is chosen instead, and the chassis
            // lets it go itself) and the key's editor
            "z10" if self.page.view == View::Keys && self.page.sel.is_some() => {
                let mut k = Vec::new();
                if ui.get_board_shown() {
                    k.push((ui.get_board_x(), ui.get_board_y(), ui.get_board_width(), ui.get_board_height()));
                }
                if ui.get_key_open() {
                    k.push((ui.get_key_tile_x(), ui.get_key_tile_y(), ui.get_key_tile_w(), ui.get_key_tile_h()));
                }
                Some(k)
            }
            // each button's words beside the picture (inside the stage, which
            // clips them), the list and the button's Key detail box
            "mouse" if self.mouse.tab == 0 && self.mouse.button.is_some() => {
                let (x0, y0, w) = (ui.get_mouse_wx(), ui.get_mouse_wy(), ui.get_mouse_w());
                let (sy, sh) = (y0 + ui.get_mouse_stage_y(), ui.get_mouse_stage_height());
                let canvas = x0 + w / 2.0 - 600.0;
                let mut k: Vec<(f32, f32, f32, f32)> = crate::mouse::CALLS
                    .iter()
                    .map(|(left, top, _)| {
                        let (lx, ly) = (canvas + if *left { 220.0 } else { 780.0 }, sy + top);
                        let (a, b) = (lx.max(x0), (lx + 200.0).min(x0 + w));
                        let (c, d) = (ly.max(sy), (ly + 40.0).min(sy + sh));
                        (a, c, (b - a).max(0.0), (d - c).max(0.0))
                    })
                    .collect();
                let (ly, lh) = (y0 + ui.get_mouse_low_top(), ui.get_mouse_low_height());
                k.push((x0, ly, 260.0, lh));
                k.push((x0 + 272.0, ly, w - 272.0, lh));
                Some(k)
            }
            // a pad on Home's desk: the whole desk (where a press chooses
            // another, carries, or draws, and on the empty desk or a device
            // lets it go itself), its bar and its zoom over it
            "home" if self.desk.pad_chosen.is_some() => Some(vec![(ui.get_desk_x(), ui.get_desk_y(), ui.get_desk_w(), ui.get_desk_h())]),
            _ => None,
        }
    }

    /// The page's chosen thing let go.
    fn let_chosen_go_now(&mut self) {
        match self.ui.get_place().as_str() {
            "z10" => {
                self.page.sel = None;
                self.listening = None;
            }
            "mouse" => self.mouse.button = None,
            "home" => {
                self.desk.pad_chosen = None;
                self.show_desk();
            }
            _ => {}
        }
        self.close_menu();
    }

    /// Escape, with nothing being typed: the page's chosen thing let go, if
    /// it has one (false when it has none, for Escape to go on to the page).
    pub fn escape(&mut self) -> bool {
        // (on Home, Escape shuts the color picker open on a pad, then lets the
        // chosen pad go, and then leaves edit mode, as Done does)
        if self.ui.get_place() == "home" && self.pick.is_some() {
            self.close_picker();
            return true;
        }
        if self.ui.get_place() == "home" && self.desk.pad_chosen.is_some() {
            self.let_chosen_go_now();
            return true;
        }
        if self.ui.get_place() == "home" && self.desk.editing {
            self.desk.set_editing(false);
            self.desk_frames_run();
            self.show_desk();
            return true;
        }
        if self.chosen_keeps().is_none() || self.menu.is_some() || self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.pick.is_some() || self.place_menu {
            return false;
        }
        self.let_chosen_go_now();
        self.refresh();
        true
    }

    fn menu_chose(&mut self, i: usize) {
        if self.profile_menu {
            let anchor = (self.ui.get_menu_ax(), self.ui.get_menu_ay(), self.ui.get_menu_ah());
            self.close_menu();
            match i {
                // Duplicate: a copy, its name open to change
                0 => {
                    self.mouse.duplicate_profile();
                    self.ui.set_mouse_renaming(true);
                }
                // Use with an app: the apps it can follow
                1 => {
                    if let Some(at) = self.mouse.profiles.as_ref().map(|s| s.active) {
                        self.open_apps_menu(1, at, anchor);
                        return;
                    }
                }
                // Delete, unless it's the only one (that row is dimmed)
                3 if self.mouse.profiles.as_ref().is_some_and(|p| p.list.len() > 1) => {
                    self.mouse.delete_profile();
                    self.show_mouse_lights();
                }
                _ => {}
            }
            self.refresh();
            return;
        }
        if self.desk_light_menu {
            self.close_menu();
            if let Some((e, ..)) = crate::desk_light::EFFECTS.get(i) {
                self.set_desk_light(*e);
            }
            self.refresh();
            return;
        }
        if self.kb_profile_menu {
            let anchor = (self.ui.get_menu_ax(), self.ui.get_menu_ay(), self.ui.get_menu_ah());
            self.close_menu();
            if i == 0 {
                self.open_apps_menu(0, self.page.editing as usize, anchor);
                return;
            }
            self.refresh();
            return;
        }
        if let Some((device, profile, anchor)) = self.apps_menu {
            // (an app ticked or not, the menu open again for the next)
            let tied = self.tied_apps(device, profile);
            let mut apps: Vec<String> = self.front_apps.list.iter().map(|a| a.id.clone()).collect();
            for t in &tied {
                if !apps.contains(t) {
                    apps.push(t.clone());
                }
            }
            if let Some(app) = i.checked_sub(1).and_then(|k| apps.get(k)).cloned() {
                self.tie_app(device, profile, &app, !tied.contains(&app));
                self.open_apps_menu(device, profile, anchor);
                self.refresh();
                return;
            }
            self.close_menu();
            self.refresh();
            return;
        }
        if let Some(k) = self.mouse.menu {
            self.close_menu();
            if let (Some((_, a)), Some(b)) = (self.mouse.choices_for(k).get(i), self.mouse.button) {
                self.mouse.set_job(b, a.clone());
            }
            self.refresh();
            return;
        }
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
        // New screen layout's menu: a blank layout, a copy of one, or one Ectodeck ships put
        // back, opened in the editor
        if let Some(made) = self.new_menu.clone() {
            self.close_menu();
            let at = match made.get(i).cloned().flatten() {
                Some(NewFrom::Scratch) => Some(self.screens.new_page()),
                Some(NewFrom::Copy(id)) => self.screens.index_of(&id).and_then(|k| self.copy_layout(k)),
                Some(NewFrom::Ready(key)) => self.screens.add_built_in(&key),
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
                let (layer, pos, d) = (m.layer, m.pos, d.clone());
                self.close_menu();
                // a macro keeps playing the way it already plays on its other keys
                let d = self.page.with_macro_play(&d);
                if self.page.sel == Some(pos) {
                    self.page.set_def(LAYERS[layer].0, pos, &d);
                }
            }
            Some(Choice::Page(p)) => {
                m.page = Some(*p);
                m.query.clear();
                self.ui.set_menu_query("".into());
                self.build_menu();
            }
            Some(Choice::Play(id)) => {
                let (layer, pos, id) = (m.layer, m.pos, id.clone());
                self.close_menu();
                if let Some(code) = self.play_code(&id) {
                    if self.page.sel == Some(pos) {
                        self.page.set_def(LAYERS[layer].0, pos, &z10::screen_slot(code));
                    }
                }
            }
            Some(Choice::Job(id)) => {
                let (layer, pos, id) = (m.layer, m.pos, format!("{}{id}", crate::jobs::ON_KEY));
                self.close_menu();
                if let Some(code) = self.play_code(&id) {
                    if self.page.sel == Some(pos) {
                        self.page.set_def(LAYERS[layer].0, pos, &z10::screen_slot(code));
                    }
                }
            }
            _ => {}
        }
    }

    /// The code a key plays playlist `id` with (see screen_keys::code_for), kept in Ectodeck's
    /// settings once the playlist has one; None while every code is some key's.
    fn play_code(&mut self, id: &str) -> Option<u32> {
        let lists = &self.screens.playlists;
        // (a code kept for one of Ectodeck's jobs is as much in use as a playlist's)
        let code = crate::screen_keys::code_for(id, &self.keyed, &self.page.sent_codes(), |p| p.starts_with(crate::jobs::ON_KEY) || lists.iter().any(|l| l.id == p))?;
        if self.keyed.get(&code).map(String::as_str) != Some(id) {
            self.keyed.insert(code, id.to_string());
            let kept: serde_json::Map<String, serde_json::Value> = self.keyed.iter().map(|(c, id)| (c.to_string(), serde_json::json!(id))).collect();
            if let Err(e) = self.settings.set("devices.z10.screen.keyed", serde_json::Value::Object(kept)) {
                self.status(format!("The key's playlist was not saved: {e}"), 2);
            }
            self.sync_playlist_keys();
        }
        Some(code)
    }

    /// The keyboard page's names for the playlists keys can play, by their codes (a code whose
    /// playlist was deleted left out).
    fn sync_playlist_keys(&mut self) {
        let lists = &self.screens.playlists;
        self.page.playlist_keys = self.keyed.iter().filter_map(|(c, id)| Some((*c, lists.iter().find(|l| &l.id == id)?.name.clone()))).collect();
        // (and the codes Ectodeck's own jobs have, in their words)
        let names = self.job_names();
        let jobs: Vec<(u32, Option<crate::jobs::Job>)> = self.keyed.iter().filter_map(|(c, id)| Some((*c, crate::jobs::Job::of(id.strip_prefix(crate::jobs::ON_KEY)?)))).collect();
        self.page.job_keys = jobs.iter().map(|(c, j)| (*c, j.as_ref().map_or_else(|| "Deleted macro".to_string(), |j| names.of(j)))).collect();
        self.page.job_prints = jobs
            .iter()
            .map(|(c, j)| {
                let print = match j {
                    Some(crate::jobs::Job::MouseProfile(crate::jobs::To::Id(id))) => names.mouse.iter().find(|(k, _)| k == id).map_or_else(|| "Deleted".to_string(), |(_, n)| n.clone()),
                    Some(crate::jobs::Job::Macro { n, .. }) => names.macros.iter().find(|(k, _)| k == n).map_or_else(|| "Deleted".to_string(), |(_, n)| n.clone()),
                    Some(j) => names.of(j),
                    None => "Deleted".into(),
                };
                (*c, print)
            })
            .collect();
    }

    /// What a key given one of Ectodeck's own jobs (`id`, see jobs.rs) does,
    /// as the note under its job says it.
    fn job_note(&self, id: &str) -> String {
        use crate::jobs::{Job, To};
        let names = self.job_names();
        let Some(j) = Job::of(id) else {
            return "The macro this key played was deleted, so the key does nothing until you give it another job.".into();
        };
        match &j {
            Job::MouseProfile(To::Id(p)) => match names.mouse.iter().find(|(k, _)| k == p) {
                Some((_, name)) => format!("Switches the mouse to {name} while Ectodeck is running."),
                None => "The mouse profile this key switched to was deleted, so the key does nothing until you give it another job.".into(),
            },
            Job::MouseProfile(To::Next) => "Switches the mouse to its next profile while Ectodeck is running.".into(),
            Job::MouseProfile(To::Previous) => "Switches the mouse to its previous profile while Ectodeck is running.".into(),
            Job::Macro { .. } => format!("Ectodeck plays {} while it’s running. With Ectodeck closed, this key does nothing.", names.of(&j)),
            Job::KeyboardProfile(_) | Job::Screen(_) => format!("{} while Ectodeck is running.", names.of(&j)),
        }
    }

    /// The job a key's action is, if it is one of Ectodeck's own (its host
    /// slot's code kept for one; see jobs.rs).
    fn key_job(&self, a: &Action) -> Option<crate::jobs::Job> {
        let code = z10::screen_job(a)?;
        crate::jobs::Job::of(self.keyed.get(&code)?.strip_prefix(crate::jobs::ON_KEY)?)
    }

    /// Whether a key's action is one of Ectodeck's own jobs' codes, a job
    /// deleted since included.
    fn is_job_key(&self, a: &Action) -> bool {
        z10::screen_job(a).and_then(|c| self.keyed.get(&c)).is_some_and(|id| id.starts_with(crate::jobs::ON_KEY))
    }

    /// What kind of job a key has, as its kind switch and the menu's pages
    /// tell it: one of Ectodeck's own, by what it does (another device's
    /// profile is a profile, Ectodeck's macro a macro).
    fn key_tag(&self, a: &Action) -> &'static str {
        match self.key_job(a) {
            Some(crate::jobs::Job::MouseProfile(_) | crate::jobs::Job::KeyboardProfile(_)) => "Profile",
            Some(crate::jobs::Job::Macro { .. }) => "Macro",
            Some(crate::jobs::Job::Screen(_)) => "Screen",
            None if self.is_job_key(a) => "Macro",
            None => z10::kind_of(a),
        }
    }

    /// Other devices' jobs, and Ectodeck's own macros, as the action menu
    /// lists them, those that switch a profile (`profiles`) or the macros:
    /// each one's row (its definition, for its tick, once it has a code),
    /// what choosing it does, and its row's kind (5 while every code is
    /// some key's).
    fn device_jobs(&self, profiles: bool) -> Vec<(Entry, Choice, i32)> {
        let sent = self.page.sent_codes();
        let lists = &self.screens.playlists;
        self.job_names()
            .for_device(crate::jobs::KEYBOARD)
            .into_iter()
            .filter(|(_, j)| matches!(j, crate::jobs::Job::Macro { .. }) != profiles)
            .map(|(words, j)| {
                let id = format!("{}{}", crate::jobs::ON_KEY, j.id());
                let code = self.keyed.iter().find(|(_, k)| **k == id).map(|(c, _)| *c);
                let can = code.is_some() || crate::screen_keys::code_for(&id, &self.keyed, &sent, |p| p.starts_with(crate::jobs::ON_KEY) || lists.iter().any(|x| x.id == p)).is_some();
                let a = Entry { label: words, act: code.map_or(Action::Raw(Vec::new()), z10::screen_slot), tag: if profiles { "Profile" } else { "Macro" } };
                if can { (a, Choice::Job(j.id()), 0) } else { (a, Choice::Nothing, 5) }
            })
            .collect()
    }

    /// The jobs that each play one playlist, as the action menu lists them: each one's row
    /// (its definition, for its tick, while the playlist has a code), what choosing it does,
    /// and its row's kind: 5, a job no key can be given now, while every code is some key's.
    fn play_jobs(&self) -> Vec<(Entry, Choice, i32)> {
        let sent = self.page.sent_codes();
        let lists = &self.screens.playlists;
        lists
            .iter()
            .map(|l| {
                let code = self.keyed.iter().find(|(_, id)| **id == l.id).map(|(c, _)| *c);
                let can = code.is_some() || crate::screen_keys::code_for(&l.id, &self.keyed, &sent, |p| lists.iter().any(|x| x.id == p)).is_some();
                // (a playlist no key plays yet has no job to tick: an empty code matches no key's)
                let a = Entry { label: format!("Play {}", l.name), act: code.map_or(Action::Raw(Vec::new()), z10::screen_slot), tag: "Screen" };
                if can { (a, Choice::Play(l.id.clone()), 0) } else { (a, Choice::Nothing, 5) }
            })
            .collect()
    }

    /// A key with a screen job pressed (its code, see z10::screen_job), as the keyboard's own
    /// reports tell of it: Record takes it as the key's job; else, while Ectodeck plays the
    /// screen, the job is done for the profile whose playlist plays. Nothing while a layout is
    /// open in the Screen Editor, the Onboard layout tab has the screen, or another program's
    /// screen shows (as a game's launch).
    /// A key with one of the keyboard's host slots pressed (`down`) or let
    /// go: one given one of Ectodeck's own jobs does it (a macro played as
    /// long as it says); a screen job is done as its key goes down.
    pub fn screen_key_event(&mut self, code: u32, down: bool) {
        let job = self.keyed.get(&code).and_then(|id| id.strip_prefix(crate::jobs::ON_KEY)).and_then(crate::jobs::Job::of);
        match job {
            // (Record takes the key as its job, as it takes a screen job's)
            Some(_) if self.listening.is_some() => {
                if down {
                    self.screen_key(code);
                }
            }
            Some(j) => self.play_for(&format!("z10/{code}"), Some(j.write()), down),
            None if down => self.screen_key(code),
            None => {}
        }
    }

    pub fn screen_key(&mut self, code: u32) {
        if let Some((layer, _)) = self.listening {
            self.assign(layer, z10::screen_slot(code));
            return;
        }
        if !self.controlled || self.editor.is_some() || self.aside || self.lcd.sequence.is_some() {
            return;
        }
        if code == z10::NEXT_LAYOUT || code == z10::PREV_LAYOUT {
            if let Some(pl) = &self.player {
                pl.tell(crate::lcd_player::Control::Step(if code == z10::NEXT_LAYOUT { 1 } else { -1 }));
            }
            return;
        }
        let n = self.page.editing;
        let count = self.screens.playlists.len();
        let mine = self.screens.of_profile(n);
        let to = match code {
            z10::NEXT_PLAYLIST => (count > 1).then(|| (mine + 1) % count),
            z10::PREV_PLAYLIST => (count > 1).then(|| (mine + count - 1) % count),
            c => self.keyed.get(&c).and_then(|id| self.screens.playlists.iter().position(|l| &l.id == id)).filter(|k| *k != mine),
        };
        if let Some(k) = to {
            let id = self.screens.playlists[k].id.clone();
            self.screens.choose(n, &id);
            self.pages_edited();
            if let Some(pl) = &self.player {
                pl.tell(crate::lcd_player::Control::Begin);
            }
            self.lcd.page = 0;
            // (the playhead as the new playlist has it: none with one layout)
            self.set_playhead();
            self.refresh();
        }
    }

    /// What a key with screen job `code` does, as the note under its job says it, naming the
    /// playlist its profile plays.
    fn screen_note(&self, code: u32) -> String {
        // (one of Ectodeck's own jobs)
        if let Some(id) = self.keyed.get(&code).and_then(|id| id.strip_prefix(crate::jobs::ON_KEY)) {
            return self.job_note(id);
        }
        let pl = &self.screens.playlist(self.page.editing).name;
        match code {
            z10::NEXT_LAYOUT => format!("Shows the next layout in {pl} while Ectodeck is running."),
            z10::PREV_LAYOUT => format!("Shows the previous layout in {pl} while Ectodeck is running."),
            z10::NEXT_PLAYLIST => format!("Plays the playlist after {pl} while Ectodeck is running."),
            z10::PREV_PLAYLIST => format!("Plays the playlist before {pl} while Ectodeck is running."),
            c => match self.page.playlist_keys.get(&c) {
                Some(name) => format!("Plays {name} while Ectodeck is running."),
                None => "The playlist this key played was deleted, so the key does nothing until you give it another job.".into(),
            },
        }
    }

    fn build_menu(&mut self) {
        let Some(m) = &self.menu else { return };
        let def = self.page.def(LAYERS[m.layer].0, m.pos);
        let g = self.page.groups();
        let lists: [&Vec<Entry>; 7] = [&g.keys, &g.media, &g.mouse, &g.profiles, &g.layouts, &g.playlists, &g.board];
        let mut rows: Vec<MenuRow> = Vec::new();
        let mut choices: Vec<Choice> = Vec::new();
        let row = |kind: i32, label: &str, tag: &str, tick: bool| MenuRow { kind, label: label.into(), tag: tag.into(), tick, sub: "".into() };
        let q = m.query.trim().to_lowercase();
        // each job as a row: what choosing it does, and the row's kind (0, or 5 for one that
        // can't be given now)
        let set = |list: &[Entry]| list.iter().map(|e| (e.clone(), Choice::Set(e.act.clone()), 0)).collect::<Vec<_>>();
        let plays = self.play_jobs();
        let (others, own_macros) = (self.device_jobs(true), self.device_jobs(false));
        let job_key = self.is_job_key(&def);
        let def_tag = self.key_tag(&def);
        // a searchable list, as pickList draws it
        let pick = |rows: &mut Vec<MenuRow>, choices: &mut Vec<Choice>, groups: &[(&str, Vec<(Entry, Choice, i32)>)]| {
            let mut n = 0;
            for (head, items) in groups {
                let hit: Vec<&(Entry, Choice, i32)> = items.iter().filter(|(a, ..)| q.is_empty() || a.label.to_lowercase().contains(&q) || a.tag.to_lowercase().contains(&q)).collect();
                if hit.is_empty() {
                    continue;
                }
                if !head.is_empty() {
                    rows.push(row(1, head, "", false));
                    choices.push(Choice::Nothing);
                }
                for (a, c, kind) in hit.into_iter().take(if q.is_empty() { 400 } else { 60 }) {
                    n += 1;
                    rows.push(row(*kind, &a.label, if q.is_empty() { "" } else { a.tag }, a.act == def));
                    choices.push(c.clone());
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
                if g.macros.is_empty() && own_macros.is_empty() {
                    rows.push(row(3, "No macros yet. Make one in the Macro Editor.", "", false));
                    choices.push(Choice::Nothing);
                } else {
                    // (the keyboard's own, then those Ectodeck plays)
                    pick(&mut rows, &mut choices, &[("Macros", set(&g.macros)), ("Ectodeck's macros", own_macros.clone())]);
                }
            }
            Some(p) => {
                let title = PAGES[p];
                let what = title.trim_start_matches("A ").trim_start_matches("An ").to_lowercase();
                placeholder = format!("Search {what}s");
                // (the playlists' own jobs after stepping through them; other devices'
                // profiles after the keyboard's, under the devices' names)
                let mut items = set(lists[p]);
                if p == PLAYLIST_PAGE {
                    items.extend(plays.iter().cloned());
                }
                if p == PROFILE_PAGE {
                    pick(&mut rows, &mut choices, &[(title, items), ("The mouse", others.clone())]);
                } else {
                    pick(&mut rows, &mut choices, &[(title, items)]);
                }
                if p == PLAYLIST_PAGE && q.is_empty() && plays.iter().any(|(.., kind)| *kind == 5) {
                    rows.push(row(3, "Keys already play seven playlists, the most they can. Give one of those keys a different job to make room.", "", false));
                    choices.push(Choice::Nothing);
                }
            }
            None => {
                placeholder = "Search keys, media, macros".to_string();
                if !q.is_empty() {
                    let mut all = set(&g.macros.iter().chain(&g.keys).chain(&g.media).chain(&g.mouse).chain(&g.profiles).chain(&g.layouts).chain(&g.playlists).chain(&g.board).cloned().collect::<Vec<_>>());
                    all.extend(plays.iter().cloned());
                    all.extend(others.iter().cloned());
                    all.extend(own_macros.iter().cloned());
                    pick(&mut rows, &mut choices, &[("", all)]);
                } else {
                    if !g.macros.is_empty() {
                        rows.push(row(1, "Macros", "", false));
                        choices.push(Choice::Nothing);
                        for a in &g.macros {
                            let same = matches!((&def, &a.act), (Action::Macro { n: x, .. }, Action::Macro { n: y, .. }) if x == y);
                            rows.push(row(0, &a.label, "", same));
                            choices.push(Choice::Set(a.act.clone()));
                        }
                        rows.push(row(2, "", "", false));
                        choices.push(Choice::Nothing);
                    }
                    // (Ectodeck's own macros after the keyboard's)
                    if !own_macros.is_empty() {
                        rows.push(row(1, "Ectodeck's macros", "", false));
                        choices.push(Choice::Nothing);
                        for (a, c, kind) in &own_macros {
                            rows.push(row(*kind, &a.label, "", a.act == def));
                            choices.push(c.clone());
                        }
                        rows.push(row(2, "", "", false));
                        choices.push(Choice::Nothing);
                    }
                    rows.push(row(1, "Other", "", false));
                    choices.push(Choice::Nothing);
                    let kind = def_tag;
                    for (p, title) in PAGES.iter().enumerate() {
                        // (the screen's two pages each ticked for their own jobs; another
                        // device's job's code is no screen job)
                        let tick = if p == LAYOUT_PAGE || p == PLAYLIST_PAGE {
                            !job_key && z10::screen_job(&def).is_some_and(|c| screen_page(c) == p)
                        } else if p == PROFILE_PAGE && job_key {
                            def_tag == "Profile"
                        } else {
                            lists[p].first().is_some_and(|a| a.tag == kind) && !matches!(def, Action::Macro { .. })
                        };
                        rows.push(row(0, &format!("{title} ›"), "", tick));
                        choices.push(Choice::Page(p));
                    }
                    rows.push(row(0, "Nothing", "", z10::is_nothing(&def)));
                    choices.push(Choice::Set(Action::None));
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
        self.sync_playlist_keys();
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
                        // (a profile tied to apps says so under its name)
                        sub: self.with_apps(&self.tied_apps(0, n as usize)).unwrap_or_else(|| p.profile_effect(n)).into(),
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
                    View::Lighting if p.drafts.get(&p.editing).is_some_and(|d| d["lighting"]["keys"]["effect"] == "custom") => k.led,
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
            let fac = p.factory("base", k.pos);
            let name = p.key_label(&k);
            ui.set_key_name(name.clone().into());
            ui.set_key_factory(format!("Factory: {}", if z10::is_nothing(&fac) { "does nothing".to_string() } else { p.describe(&fac) }).into());
            ui.set_reset_label(format!("Reset {name} to factory").into());
            let layers: Vec<LayerItem> = LAYERS
                .iter()
                .enumerate()
                .map(|(i, (layer, title))| {
                    let d = p.def(layer, k.pos);
                    let mac = if let Action::Macro { n, .. } = d { Some(n) } else { None };
                    let label = if let Some(n) = mac { p.macro_label(n) } else if z10::is_nothing(&d) { "Nothing".into() } else { p.describe(&d) };
                    let held: Vec<SharedString> = mac.map(|n| p.macro_keys_held(n).into_iter().map(Into::into).collect()).unwrap_or_default();
                    let is_macro = mac.is_some();
                    LayerItem {
                        title: (*title).into(),
                        kind: z10::kind_of(&d).into(),
                        label: label.into(),
                        nothing: z10::is_nothing(&d),
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
            let fn_unset = self.layer == 1 && d == p.def("base", k.pos);
            let job = if fn_unset {
                "Same as on Base".to_string()
            } else if let Action::Macro { n, .. } = d {
                p.macro_label(n)
            } else if z10::is_nothing(&d) {
                "Nothing".into()
            } else {
                p.describe(&d)
            };
            ui.set_key_cap(lib_cap(&k.id, &name).into());
            ui.set_key_state((if rebound { format!("Rebound on {}.", p.profile_name(p.editing)) } else { "As it came from the factory.".to_string() }).into());
            ui.set_key_rebound(rebound);
            ui.set_layer_rebound(rebound_here);
            ui.set_key_kind(match self.key_tag(&d) {
                "Key" => 0,
                "Media" => 1,
                "Mouse" => 2,
                "Macro" => 3,
                "Profile" => 4,
                "Screen" => 5,
                "Keyboard" => 6,
                _ => 7,
            });
            ui.set_key_job(job.into());
            ui.set_key_listening(self.listening.is_some_and(|(l, _)| l == self.layer));
            let note = if fn_unset {
                "On the Function layer this key does what it does on Base, until you choose a job here.".to_string()
            } else {
                z10::screen_job(&d).map_or(String::new(), |c| self.screen_note(c))
            };
            ui.set_key_note(note.into());
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
                            if let Action::Macro { n, .. } = d { p.macro_label(n) } else if z10::is_nothing(&d) { "Nothing".into() } else { p.describe(&d) }
                        } else if g == "Buttons" && !z10::is_nothing(&fac) && p.describe(&fac) != name {
                            p.describe(&fac)
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
        self.desk_meet();
        self.desk.resize(self.ui.get_desk_w(), self.ui.get_desk_h());
        if self.ui.get_place() == "home" {
            self.show_desk();
        }
        if self.ui.get_place() == "mouse" {
            self.refresh_mouse();
        } else if self.ui.get_place() == "settings" {
            // what the kernel says of the keyboard, looked at every two seconds (it
            // scans every process), not at every step of a slider
            if self.dev_status.as_ref().is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(2)) {
                self.dev_status = Some((Instant::now(), crate::devices::z10()));
            }
            let dc = self.dev_status.as_ref().map(|(_, v)| v.clone()).unwrap_or_default();
            let sensors = self.sensors_view();
            let st = crate::ecto_settings::State {
                sensors: Some(sensors),
                status: dc,
                control: self.controlled,
                settings: self.settings.all(),
                report: self.weather.report(),
                locale: &crate::locale(),
                top_peak: self.levels.1,
                reference: self.levels.0,
                place_text: &self.place_text,
                mouse: self.mouse.known().then(|| {
                    if self.mouse_status.as_ref().is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(2)) {
                        self.mouse_status = Some((Instant::now(), crate::devices::sensei310()));
                    }
                    (self.mouse_status.as_ref().map(|(_, v)| v.clone()).unwrap_or_default(), self.mouse.present())
                }),
            };
            let (cells, h, map) = crate::ecto_settings::page(&st);
            self.sensor_map = map;
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
        if self.macros.is_some() {
            self.macros_elsewhere();
        }
        if let Some(m) = &mut self.macros {
            m.refresh(&self.page);
        }
    }

    /// Where Ectodeck's macros run, for the Macro Editor's library of them:
    /// each mouse profile's buttons given one, and how each plays there.
    pub fn macros_elsewhere(&mut self) {
        let Some(m) = &mut self.macros else { return };
        let (mut runs, mut names) = (Vec::new(), Vec::new());
        if let Some(store) = &self.mouse.profiles {
            for (pi, p) in store.list.iter().enumerate() {
                names.push(p.name.clone());
                for (bi, (id, label, _)) in crate::mouse::BUTTONS.iter().enumerate() {
                    if let Some(Action::Macro { n, plays }) = Action::read(&p.body["keys"]["base"][*id]) {
                        runs.push(crate::macros::Elsewhere { profile: pi as u8, control: format!("The mouse’s {}", label.to_lowercase()), at: bi as u32, n, plays });
                    }
                }
            }
        }
        // the keyboard's keys given one, by the codes Ectodeck keeps for its jobs, on its
        // profiles (named after the mouse's)
        if self.page.dev.is_some() {
            let first = names.len();
            for (k, &p) in z10::PROFILE_NUMBERS.iter().enumerate() {
                names.push(format!("keyboard’s {}", self.page.profile_name(p)));
                let Some(d) = self.page.drafts.get(&p) else { continue };
                for (layer, words) in [("base", ""), ("fn", "Fn + ")] {
                    for pos in 0..ectotype_core::protocol::NKEYS as u32 {
                        let Some(code) = Action::read(&d["keys"][layer][self.page.id(pos)]).as_ref().and_then(z10::screen_job) else { continue };
                        let job = self.keyed.get(&code).and_then(|id| id.strip_prefix(crate::jobs::ON_KEY)).and_then(crate::jobs::Job::of);
                        if let Some(crate::jobs::Job::Macro { n, plays }) = job {
                            let key = self.page.key(pos).map(|k| self.page.key_label(k)).unwrap_or_else(|| format!("Key {pos}"));
                            runs.push(crate::macros::Elsewhere { profile: (first + k) as u8, control: format!("The keyboard’s {words}{key}"), at: 1000 + pos, n, plays });
                        }
                    }
                }
            }
        }
        for ed in [&mut m.ed, &mut m.other] {
            if ed.lib == crate::macros::Lib::Ectodeck {
                ed.elsewhere = runs.clone();
                ed.names = names.clone();
            }
        }
    }

    /// How one of Ectodeck's macros plays, set in the Macro Editor, on every
    /// mouse button that runs it, on every profile.
    pub fn macro_plays(&mut self, n: u32, how: ectodeck_protocol::Plays) {
        // (the keyboard's keys that run it, kept against their codes)
        let job = format!("{}{}", crate::jobs::ON_KEY, crate::jobs::Job::Macro { n, plays: how }.id());
        self.rekey(|id| (matches!(crate::jobs::Job::of(id.strip_prefix(crate::jobs::ON_KEY)?), Some(crate::jobs::Job::Macro { n: m, .. }) if m == n)).then(|| job.clone()));
        let Some(store) = &mut self.mouse.profiles else { return };
        // (the mouse knows the buttons only as host slots: nothing to tell it)
        crate::mouse::rewrite_macro(store, n, &Action::Macro { n, plays: how }.write());
        self.macros_elsewhere();
        self.refresh();
    }

    /// Ectodeck's jobs kept against the keyboard's codes, each one `f` gives
    /// another id changed to it, and kept so.
    fn rekey(&mut self, f: impl Fn(&str) -> Option<String>) {
        let mut changed = false;
        for id in self.keyed.values_mut() {
            if let Some(new) = f(id) {
                changed |= *id != new;
                *id = new;
            }
        }
        if changed {
            let kept: serde_json::Map<String, serde_json::Value> = self.keyed.iter().map(|(c, id)| (c.to_string(), serde_json::json!(id))).collect();
            if let Err(e) = self.settings.set("devices.z10.screen.keyed", serde_json::Value::Object(kept)) {
                self.status(format!("The keys' jobs were not saved: {e}"), 2);
            }
            self.sync_playlist_keys();
        }
    }

    /// One of Ectodeck's macros deleted: every mouse button that ran it, on
    /// every profile, does nothing now, as the delete question said, so a new
    /// macro given its number later isn't run by them unasked.
    pub fn macro_gone(&mut self, n: u32) {
        // (a key that ran it does nothing now, as a button does)
        let gone = format!("{}deleted", crate::jobs::ON_KEY);
        self.rekey(|id| (matches!(crate::jobs::Job::of(id.strip_prefix(crate::jobs::ON_KEY)?), Some(crate::jobs::Job::Macro { n: m, .. }) if m == n)).then(|| gone.clone()));
        let Some(store) = &mut self.mouse.profiles else { return };
        let off = serde_json::json!({ "off": true });
        // (the profile in use is on the mouse: its buttons are told too)
        for bi in crate::mouse::rewrite_macro(store, n, &off) {
            self.mouse.set_job(bi, off.clone());
        }
        self.macros_elsewhere();
        self.refresh();
    }

    /// A control given a job Ectodeck does (`job`), pressed or let go: one of
    /// Ectodeck's macros played, as its job says it plays.
    fn play_for(&mut self, control: &str, job: Option<serde_json::Value>, down: bool) {
        let Some(j) = job.as_ref().and_then(crate::jobs::Job::read) else { return };
        let crate::jobs::Job::Macro { n, plays } = j else {
            self.run_job(&j, down);
            return;
        };
        let player = self.typist.get_or_insert_with(crate::typist::Player::start);
        if !down {
            player.up(control);
            return;
        }
        if let Some(m) = crate::library::get(n) {
            player.down(control, m["steps"].as_array().cloned().unwrap_or_default(), plays);
        }
    }

    /// One of Ectodeck's own jobs, its control pressed (`down`) or let go:
    /// another device's profile switched, or one of the keyboard's screen
    /// jobs done, as the control goes down. (A macro plays in play_for.)
    fn run_job(&mut self, j: &crate::jobs::Job, down: bool) {
        use crate::jobs::{Job, To};
        use ectodeck_protocol::Jump;
        if !down {
            return;
        }
        eprintln!("ectodeck-native: a job: {}", self.job_names().of(j));
        match j {
            Job::KeyboardProfile(to) => {
                if self.page.dev.is_none() {
                    return;
                }
                let now = self.page.active() as usize;
                let n = match to {
                    Jump::To(n) => *n as usize,
                    Jump::Next => z10::PROFILE_NUMBERS[crate::jobs::stepped(now.saturating_sub(1), z10::PROFILE_NUMBERS.len(), true)] as usize,
                    Jump::Previous => z10::PROFILE_NUMBERS[crate::jobs::stepped(now.saturating_sub(1), z10::PROFILE_NUMBERS.len(), false)] as usize,
                };
                if z10::PROFILE_NUMBERS.contains(&(n as u8)) && n != now {
                    self.choose_profile(n as u8);
                    self.refresh();
                }
            }
            Job::MouseProfile(to) => {
                let Some(store) = &self.mouse.profiles else { return };
                let i = match to {
                    To::Id(id) => store.list.iter().position(|p| &p.id == id),
                    To::Next => Some(crate::jobs::stepped(store.active, store.list.len(), true)),
                    To::Previous => Some(crate::jobs::stepped(store.active, store.list.len(), false)),
                };
                if let Some(i) = i {
                    self.mouse.choose_profile(i);
                    self.show_mouse_lights();
                    self.refresh();
                }
            }
            Job::Screen(code) => self.screen_key(*code),
            Job::Macro { .. } => {}
        }
    }

    /// The names other devices' jobs are said by, as the pages have them now:
    /// the keyboard's profiles, the mouse's, Ectodeck's macros.
    pub fn job_names(&self) -> crate::jobs::Names {
        let keyboard = if self.page.dev.is_some() { z10::PROFILE_NUMBERS.iter().map(|&n| (n, self.page.profile_name(n))).collect() } else { Vec::new() };
        let mouse = self.mouse.profiles.as_ref().map(|s| s.list.iter().map(|p| (p.id.clone(), p.name.clone())).collect()).unwrap_or_default();
        let all = crate::library::all();
        let mut macros: Vec<(u32, String)> = all.as_object().map(|m| m.iter().filter_map(|(n, v)| Some((n.parse().ok()?, v["name"].as_str().filter(|s| !s.is_empty()).map_or_else(|| format!("Macro {n}"), String::from)))).collect()).unwrap_or_default();
        macros.sort();
        crate::jobs::Names { keyboard, mouse, macros }
    }

    /// What the mouse's core says, from the mouse's thread.
    pub fn mouse_heard(&mut self, h: crate::mouse::Heard) {
        use crate::mouse::Heard;
        match h {
            Heard::State(v) => {
                self.mouse.heard_state(v);
                self.mouse.failed = None;
            }
            Heard::Failed(e) => self.mouse.failed = Some(e),
            // the picture plays the light's effect from the moment the mouse took it
            Heard::Played(light, v, at) => {
                self.mouse.played(&light, &v, at);
                self.show_mouse_lights();
                self.mouse_frames_run();
                return;
            }
            Heard::Drawn(w, scale, l) => {
                self.mouse_drawn(w, scale, *l);
                return;
            }
            // a button given one of Ectodeck's macros: pressed plays it (let go ends
            // one that plays while it's held)
            Heard::Input(control, down) => {
                self.play_for(&format!("mouse/{control}"), crate::mouse::BUTTONS.iter().position(|(b, ..)| *b == control).map(|i| self.mouse.job(i)), down);
                return;
            }
        }
        self.ui.set_mouse_known(self.mouse.known());
        self.ui.set_mouse_conn(if !self.mouse.known() { 0 } else if self.mouse.present() { 1 } else { 2 });
        // the picture drawn ahead of the page's being opened
        if self.mouse.known() {
            self.mouse_draw();
        }
        self.refresh();
    }

    /// The mouse's picture in layers for its page at the window's scale,
    /// unless it is drawn or being drawn.
    fn mouse_draw(&mut self) {
        let scale = self.scale;
        if self.mouse_layers.as_ref().is_some_and(|(s, _)| *s == scale) || self.mouse_drawing == Some(scale) {
            return;
        }
        self.mouse_drawing = Some(scale);
        self.mouse_draw_at((240.0 * scale).round() as u32);
    }

    /// The mouse's picture in layers, the mouse `w` pixels wide: on a thread of
    /// its own, or here when the page has no mouse's thread to hear it back
    /// from (a snapshot).
    fn mouse_draw_at(&mut self, w: u32) {
        let scale = self.scale;
        let draw = move || {
            let art = std::fs::read_to_string(crate::cores::dir().join("steelseries/art/sensei-310.svg")).map_err(|e| e.to_string())?;
            crate::mouse_art::layers(&art, w, &crate::stage_shadow(scale), crate::STAGE_SHADOW_OPACITY)
        };
        match self.mouse.tell.clone() {
            Some(tell) => {
                std::thread::spawn(move || match draw() {
                    Ok(l) => {
                        let _ = tell.send(crate::mouse::Heard::Drawn(w, scale, Box::new(l)));
                    }
                    Err(e) => eprintln!("ectodeck-native: the mouse's picture: {e}"),
                });
            }
            None => match draw() {
                Ok(l) => self.mouse_drawn(w, scale, l),
                Err(e) => eprintln!("ectodeck-native: the mouse's picture: {e}"),
            },
        }
    }

    /// The mouse's picture drawn, `w` pixels wide: its layers put on its page,
    /// or on the desk, whichever it was drawn for.
    fn mouse_drawn(&mut self, w: u32, scale: f32, l: crate::mouse_art::Layers) {
        let page = (240.0 * scale).round() as u32;
        if w == page && self.mouse_drawing == Some(scale) {
            self.mouse_drawing = None;
        }
        if self.desk_mouse_drawing == Some(w) {
            self.desk_mouse_drawing = None;
        }
        // (drawn for a scale the window has left: drawn again for this one)
        if scale != self.scale {
            self.mouse_draw();
            return;
        }
        let for_desk = w != page;
        let imgs = mouse_images(l, scale);
        if for_desk {
            self.desk_mouse = Some((w, imgs));
            self.show_desk_mouse();
            self.desk_light_up();
            return;
        }
        self.ui.set_mouse_picture(imgs.base.clone());
        self.ui.set_mouse_pic_m(imgs.layers.margin as f32 / scale);
        // (its lights in HDR, as the core says they are made)
        let units_mm = self.mouse.state["device"]["scale"].as_f64().map_or(520.0 / 125.1, |v| v as f32);
        self.mouse_px_mm = crate::mouse_art::px_mm(w, units_mm);
        self.mouse_hdr = Some((scale, crate::mouse_art::Hdr::new(&imgs.layers, &self.mouse_light_model(), self.mouse_px_mm)));
        self.mouse_lit = None;
        // (in the drawing's order: see mouse_art::GROUPS)
        self.ui.set_mouse_logo_under(imgs.lights[0].clone());
        self.ui.set_mouse_wheel_under(imgs.lights[1].clone());
        self.ui.set_mouse_logo(imgs.lights[2].clone());
        self.ui.set_mouse_wheel(imgs.lights[3].clone());
        self.mouse_layers = Some((scale, imgs));
        // (what the picture shows, shown again on the new layers)
        self.mouse_shows = (Some(0), Some(0));
        self.show_mouse_picked();
        self.show_mouse_lights();
        self.show_desk_mouse();
    }

    /// The chosen button picked out on the picture, and the one under the
    /// pointer a shade lighter, on Buttons.
    fn show_mouse_picked(&mut self) {
        let chosen = self.mouse.button.filter(|_| self.mouse.tab == 0).map(|b| b as u8 + 1);
        let hover = self.mouse_hover.filter(|h| self.mouse.tab == 0 && Some(*h) != chosen);
        if self.mouse_layers.is_none() || self.mouse_shows == (chosen, hover) {
            return;
        }
        self.mouse_shows = (chosen, hover);
        // (the HDR picture with them picked out)
        self.mouse_light_up();
        let imgs = &self.mouse_layers.as_ref().unwrap().1;
        if let Some(p) = chosen.and_then(|n| imgs.chosen.get(n as usize - 1)) {
            self.ui.set_mouse_picked(p.clone());
        }
        self.ui.set_mouse_picked_shown(chosen.is_some());
        if let Some(p) = hover.and_then(|n| imgs.shapes.get(n as usize - 1)) {
            self.ui.set_mouse_over(p.clone());
        }
        self.ui.set_mouse_over_shown(hover.is_some());
    }

    /// Each light on the picture in its color of the moment, as the mouse shows it.
    fn show_mouse_lights(&mut self) {
        self.mouse_light_up();
        if self.ui.get_place() == "home" {
            self.desk_light_up();
        }
        let now = Instant::now();
        for i in 0..2 {
            let (c, k) = crate::mouse_art::level(self.mouse.color_at(i, now));
            let tint = Color::from_argb_u8((k * 255.0).round() as u8, c[0], c[1], c[2]);
            if i == 0 {
                self.ui.set_mouse_logo_tint(tint);
            } else {
                self.ui.set_mouse_wheel_tint(tint);
            }
        }
    }

    /// The mouse's picture drawn for the window's scale now, if it isn't (here,
    /// with no thread to draw it on: a snapshot).
    pub fn ready_mouse_picture(&mut self) {
        self.mouse_draw();
    }

    /// The mouse's picture drawn for its size on Home's desk now, settled or
    /// not, as the window draws it once the desk settles (a snapshot has no
    /// moment to wait for that).
    pub fn ready_desk_mouse(&mut self) {
        let Some(i) = self.desk.items.iter().position(|it| it.id == "mouse") else { return };
        self.mouse_draw();
        let w = ((self.desk.screen(i)[2] * self.scale / 4.0).round() * 4.0).clamp(32.0, 1600.0) as u32;
        if w != (240.0 * self.scale).round() as u32 && self.desk_mouse.as_ref().is_none_or(|(dw, _)| *dw != w) {
            self.desk_mouse_drawing = Some(w);
            self.mouse_draw_at(w);
        }
    }

    /// How the mouse's lights are made: as the Key lab tunes them, or as its
    /// core's light file has them.
    pub fn mouse_light_model(&self) -> serde_json::Value {
        self.mouse_light.clone().unwrap_or_else(|| crate::mouse::light_model(&self.mouse.state["device"]))
    }

    /// The mouse lit as the Key lab tunes it: its lamps made again for that.
    pub fn set_mouse_light(&mut self, light: serde_json::Value) {
        self.mouse_light = Some(light);
        if let Some((scale, imgs)) = &self.mouse_layers {
            self.mouse_hdr = Some((*scale, crate::mouse_art::Hdr::new(&imgs.layers, &self.mouse_light_model(), self.mouse_px_mm)));
        }
        self.desk_mouse_hdr = None;
        self.mouse_light_up();
    }

    /// The layers Home's desk shows the mouse in: those drawn for its size
    /// there, or the page's while those aren't drawn; and their width.
    fn desk_layers(&self) -> Option<(u32, &crate::mouse_art::Layers)> {
        match (&self.desk_mouse, &self.mouse_layers) {
            (Some((w, imgs)), _) => Some((*w, &imgs.layers)),
            (None, Some((scale, imgs))) => Some(((240.0 * scale).round() as u32, &imgs.layers)),
            _ => None,
        }
    }

    /// The mouse lit in HDR for Home's desk, as it shows now: dark and half
    /// shown while it isn't plugged in.
    fn desk_light_up(&mut self) {
        let Some(item) = self.desk.items.iter().find(|it| it.id == "mouse") else { return };
        let on = item.connected;
        let (white, ceiling) = (self.levels.0 as f32, self.light_gain(self.levels.0));
        let now = Instant::now();
        let colors = [0, 1].map(|i| self.mouse_color(i, now).filter(|_| on));
        let light = self.mouse_light_model();
        let units_mm = self.mouse.state["device"]["scale"].as_f64().map_or(520.0 / 125.1, |v| v as f32);
        let Some((w, layers)) = self.desk_layers() else { return };
        if self.desk_mouse_hdr.as_ref().is_none_or(|(hw, _)| *hw != w) {
            self.desk_mouse_hdr = Some((w, crate::mouse_art::Hdr::new(layers, &light, crate::mouse_art::px_mm(w, units_mm))));
        }
        // (the same layers, borrowed field by field beside the lamps)
        let layers = match (&self.desk_mouse, &self.mouse_layers) {
            (Some((_, imgs)), _) | (None, Some((_, imgs))) => &imgs.layers,
            _ => return,
        };
        let Some((_, hdr)) = self.desk_mouse_hdr.as_mut() else { return };
        let lit = hdr.on_ground(layers, colors, if on { 1.0 } else { 0.5 }, white, ceiling);
        self.desk_mouse_lit = Some(Rc::new(lit));
    }

    /// Each of the mouse's lights at its brightest now, in nits: its window
    /// over its LED in the light's color of the moment, no brighter than the
    /// screen shows (Peak nits); 0 for a light that's off.
    pub fn mouse_brightest(&self) -> [f64; 2] {
        let Some((_, hdr)) = &self.mouse_hdr else { return [0.0; 2] };
        let now = Instant::now();
        let most = self.light_gain(self.levels.0) as f64 * self.levels.0;
        std::array::from_fn(|i| {
            let Some(c) = self.mouse_color(i, now) else { return 0.0 };
            let lin = |v: u8| (v as f64 / 255.0).powf(2.2);
            let y = 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
            hdr.lamps.get(i).map_or(0.0, |l| (l.peak() as f64 * y).min(most))
        })
    }

    /// The mouse's lights as the Key lab chooses them, one at a time: each
    /// where its window is on the drawing, by its id and name. None before
    /// the mouse's picture is drawn.
    pub fn mouse_lab_parts(&self) -> Option<Vec<ectodeck_draw::hdr::CapShape>> {
        let (scale, imgs) = self.mouse_layers.as_ref()?;
        let boxes = crate::mouse_art::light_boxes(&imgs.layers, (240.0 * scale).round() as u32);
        let names = self.mouse.state["device"]["capabilities"]["lighting"]["lights"].as_array().cloned().unwrap_or_default();
        Some(
            crate::mouse_art::LIGHT_IDS
                .iter()
                .zip(boxes)
                .map(|(id, b)| {
                    let label = names.iter().find(|l| l["id"] == *id).and_then(|l| l["label"].as_str().map(String::from)).unwrap_or_else(|| id.to_string());
                    ectodeck_draw::hdr::CapShape { id: id.to_string(), label, short: None, foot: b, lit: true, legend: false, button: false, face: b, print: None, bump: Default::default() }
                })
                .collect(),
        )
    }

    /// The mouse lit in HDR for its page's hole, as it shows now: its chosen
    /// and hovered buttons, its lights in their colors of the moment, as
    /// bright as their LEDs make them, up to Peak nits.
    fn mouse_light_up(&mut self) {
        let (white, ceiling) = (self.levels.0 as f32, self.light_gain(self.levels.0));
        let now = Instant::now();
        let colors = [0, 1].map(|i| self.mouse_color(i, now));
        let Some((scale, imgs)) = &self.mouse_layers else { return };
        let Some((_, hdr)) = self.mouse_hdr.as_mut().filter(|(s, _)| s == scale) else { return };
        let part = mouse_part(&self.ui, *scale, &imgs.layers);
        let lit = hdr.picture(&imgs.layers, [0x1b, 0x1b, 0x1f], self.mouse_shows.0, self.mouse_shows.1, colors, white, ceiling, part);
        self.mouse_lit = Some(Rc::new(lit));
        self.mouse_part = part;
        self.ui.set_mouse_hdr_ready(true);
    }

    /// Where the mouse goes in the window, in its pixels (`s` to a logical
    /// pixel), and its picture there: on its page, in the stage's hole, while
    /// the page shows its picture; nothing otherwise. Drawn again first if
    /// the hole shows another part of it now (the window's width changed).
    pub fn mouse_shown(&mut self, s: f32) -> ((i32, i32, usize, usize), Option<Rc<crate::picture::Lit>>) {
        let none = ((0, 0, 0, 0), None);
        // on Home's desk: where the desk has it, with its shadow round it
        if self.ui.get_place() == "home" {
            let Some(i) = self.desk.items.iter().position(|it| it.id == "mouse") else { return none };
            if self.desk_mouse_lit.is_none() {
                self.desk_light_up();
            }
            let Some((dw, layers)) = self.desk_layers() else { return none };
            let [x, y, w, h] = self.desk.screen(i);
            // (its margin at the size it shows: as drawn, scaled as the desk shows it)
            let k = w / (dw as f32 / self.scale);
            let m = layers.margin as f32 / self.scale * k;
            let (px, py) = (((self.ui.get_desk_x() + x - m) * s).round() as i32, ((self.ui.get_desk_y() + y - m) * s).round() as i32);
            let (pw, ph) = (((w + 2.0 * m) * s).round() as usize, ((h + 2.0 * m) * s).round() as usize);
            let lit = self.desk_mouse_lit.clone();
            // (drawn for its size there, as it is: no stretching a pixel or two)
            let (pw, ph) = match &lit {
                Some(l) if l.w.abs_diff(pw) <= 4 && l.h.abs_diff(ph) <= 4 && dw == ((w * self.scale / 4.0).round() * 4.0) as u32 => (l.w, l.h),
                _ => (pw, ph),
            };
            return ((px, py, pw, ph), lit);
        }
        if !self.ui.get_mouse_hole_shown() {
            return none;
        }
        let Some((scale, imgs)) = &self.mouse_layers else { return none };
        if (*scale - s).abs() > 1e-3 {
            // (drawn for another scale: the stage's shade fills the hole till this one's is)
            self.ui.set_mouse_hdr_ready(false);
            return none;
        }
        let part = mouse_part(&self.ui, *scale, &imgs.layers);
        if part != self.mouse_part || self.mouse_lit.is_none() {
            self.mouse_light_up();
        }
        let (px, py) = ((self.ui.get_mouse_pic_wx() * s).round() as i32, (self.ui.get_mouse_pic_wy() * s).round() as i32);
        ((px + part.0 as i32, py + part.1 as i32, part.2, part.3), self.mouse_lit.clone())
    }

    /// The lights' effects played on the picture, 30 frames a second, while
    /// the picture shows and an effect moves.
    fn mouse_frames_run(&mut self) {
        let want = self.mouse_pictured() && self.mouse.moving();
        if want == self.mouse_frames.is_some() {
            return;
        }
        self.mouse_frames = want.then(|| {
            let t = slint::Timer::default();
            let asks = self.asks.clone();
            t.start(slint::TimerMode::Repeated, Duration::from_millis(33), move || asks.borrow_mut().push(Ask::MouseFrame));
            t
        });
    }

    /// What is on the mouse's picture at a point on it (in its logical
    /// pixels): the button there, on Buttons, or the light, on Lighting.
    fn mouse_pic_at(&self, x: f32, y: f32) -> (Option<u8>, Option<usize>) {
        let Some((scale, imgs)) = &self.mouse_layers else { return (None, None) };
        let (px, py) = (x * scale, y * scale);
        match self.mouse.tab {
            0 => (imgs.layers.button_at(px, py), None),
            1 => (None, imgs.layers.light_at(px, py)),
            _ => (None, None),
        }
    }

    /// The same at a point in the window, inside the stage, which clips the picture.
    fn mouse_pic_at_window(&self, x: f32, y: f32) -> (Option<u8>, Option<usize>) {
        let ui = &self.ui;
        let (x0, y0, w) = (ui.get_mouse_wx(), ui.get_mouse_wy(), ui.get_mouse_w());
        let (sy, sh) = (y0 + ui.get_mouse_stage_y(), ui.get_mouse_stage_height());
        if x < x0 || x >= x0 + w || y < sy || y >= sy + sh {
            return (None, None);
        }
        let m = ui.get_mouse_pic_m();
        self.mouse_pic_at(x - (x0 + w / 2.0 - 600.0 + 480.0 - m), y - (sy + 30.0 - m))
    }

    /// Whether the mouse's picture shows: on its page's Buttons or Lighting,
    /// or on Home's desk.
    fn mouse_pictured(&self) -> bool {
        match self.ui.get_place().as_str() {
            "mouse" => self.mouse.tab < 2,
            "home" => self.desk.items.iter().any(|i| i.id == "mouse"),
            _ => false,
        }
    }

    // ------------------------------------------------------------ Home's desk

    /// The devices Ectodeck knows, on the desk: those it has read or met, and
    /// those it has kept a place for; each at its size, and whether it is
    /// plugged in.
    fn desk_meet(&mut self) {
        if self.page.dev.is_some() || self.desk.keeps("z10") {
            let core = crate::cores::shown();
            let size = crate::desk::size_of(&core.art, core.device["light"]["scale"].as_f64().unwrap_or(1.38) as f32);
            // (not yet known to be plugged in, it is drawn as plugged in: only a keyboard gone shows dimmed)
            self.desk.meet("z10", core.name(), "z10", size, self.ui.get_conn_state() != 2, true);
        }
        if self.mouse.known() || self.desk.keeps("mouse") {
            let dev = &self.mouse.state["device"];
            let v = crate::mouse_art::VIEW;
            let per = dev["scale"].as_f64().unwrap_or(520.0 / 125.1) as f32;
            let size = (v.2 / per * crate::desk::PER_MM, v.3 / per * crate::desk::PER_MM);
            let name = dev["name"].as_str().unwrap_or("SteelSeries Sensei 310").to_string();
            self.desk.meet("mouse", &name, "mouse", size, self.mouse.present() || self.mouse.state.is_null(), false);
        }
    }

    /// The desk as it is now: its devices where the view puts them, edit
    /// mode's parts, and the mouse's picture.
    fn show_desk(&mut self) {
        let ui = &self.ui;
        self.desk.resize(ui.get_desk_w(), ui.get_desk_h());
        let names = self.desk_names();
        let d = &self.desk;
        let items: Vec<DeskItem> = d
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let [x, y, w, h] = d.screen(i);
                let backed = d.name_backed(names[i]);
                DeskItem { kind: if it.id == "z10" { 0 } else { 1 }, name: it.name.as_str().into(), sub: if it.connected { "".into() } else { "Not connected".into() }, x, y, w, h, off: !it.connected, hover: d.hover == Some(i), backed }
            })
            .collect();
        show_rows(ui.get_desk_items(), |r| ui.set_desk_items(r), items);
        let r = crate::desk::PAD_RADIUS * d.view.z;
        let pads: Vec<DeskPad> = d.pads_shown().into_iter().map(|([x, y, w, h], chosen, hover, drawing)| DeskPad { x, y, w, h, r, chosen, hover, drawing }).collect();
        show_rows(ui.get_desk_pads(), |r| ui.set_desk_pads(r), pads);
        // (what the desk said of a picture is said while its pad is chosen)
        if d.pad_chosen.is_none() {
            self.pad_note.clear();
        }
        let d = &self.desk;
        let chosen = d.pad_chosen.and_then(|i| d.pads.get(i));
        ui.set_desk_pad_chosen(d.pad_chosen.filter(|_| chosen.is_some()).map_or(-1, |i| i as i32));
        if let Some(p) = chosen {
            ui.set_desk_pad_rgb(color(p.color));
        }
        ui.set_desk_pad_pictured(chosen.is_some_and(|p| p.picture.is_some()));
        ui.set_desk_pad_note(self.pad_note.as_str().into());
        ui.set_desk_editing(d.editing);
        ui.set_desk_panning(d.panning());
        ui.set_desk_carrying(d.carrying());
        ui.set_desk_zoom(d.zoom_text().into());
        ui.set_desk_empty(if d.items.is_empty() { crate::desk::works_with().into() } else { "".into() });
        self.show_desk_mouse();
        // (the mouse's lights, and their effects playing, on the desk too)
        self.show_mouse_lights();
        self.mouse_frames_run();
    }

    /// The mouse on the desk: its picture as drawn for its size there, or its
    /// page's shown at that size while that one is drawn.
    fn show_desk_mouse(&mut self) {
        let Some(i) = self.desk.items.iter().position(|it| it.id == "mouse") else { return };
        let shown = self.desk.screen(i)[2];
        let scale = self.scale;
        let imgs = match (&self.desk_mouse, &self.mouse_layers) {
            (Some((w, imgs)), _) => Some((*w as f32 / scale, imgs)),
            (None, Some((_, imgs))) => Some((240.0, imgs)),
            _ => None,
        };
        if let Some((drawn, imgs)) = imgs {
            let ui = &self.ui;
            ui.set_desk_mouse_picture(imgs.base.clone());
            ui.set_desk_mouse_m(imgs.layers.margin as f32 / scale);
            ui.set_desk_mouse_k(shown / drawn);
            ui.set_desk_mouse_logo_under(imgs.lights[0].clone());
            ui.set_desk_mouse_wheel_under(imgs.lights[1].clone());
            ui.set_desk_mouse_logo(imgs.lights[2].clone());
            ui.set_desk_mouse_wheel(imgs.lights[3].clone());
        }
    }

    /// Once the desk has settled, the mouse's picture drawn for its size on
    /// it (to the nearest four pixels), unless it is drawn or being drawn.
    fn desk_mouse_want(&mut self) {
        let Some(i) = self.desk.items.iter().position(|it| it.id == "mouse") else { return };
        if !self.desk_settled() || self.mouse_layers.is_none() {
            return;
        }
        let w = ((self.desk.screen(i)[2] * self.scale / 4.0).round() * 4.0).clamp(32.0, 1600.0) as u32;
        let page = (240.0 * self.scale).round() as u32;
        if w == page {
            if self.desk_mouse.take().is_some() {
                self.show_desk_mouse();
            }
            return;
        }
        if self.desk_mouse.as_ref().is_some_and(|(dw, _)| *dw == w) || self.desk_mouse_drawing == Some(w) {
            return;
        }
        self.desk_mouse_drawing = Some(w);
        self.mouse_draw_at(w);
    }

    /// The desk's easing and fading, a frame each sixtieth of a second while they go on.
    fn desk_frames_run(&mut self) {
        let want = self.desk.animating();
        if want == self.desk_frames.is_some() {
            return;
        }
        self.desk_frames = want.then(|| {
            let t = slint::Timer::default();
            let asks = self.asks.clone();
            t.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || asks.borrow_mut().push(Ask::DeskTick));
            t
        });
    }

    /// Whether the desk has come to rest: nothing easing or carried, and the
    /// view the same for a moment, so pictures drawn for its sizes are worth drawing.
    pub fn desk_settled(&self) -> bool {
        !self.desk.animating() && !self.desk.panning() && !self.desk.carrying() && self.desk.changed.elapsed() >= Duration::from_millis(150)
    }

    /// Where the keyboard is on Home's desk, in the window's logical pixels, while Home shows it.
    pub fn desk_keyboard(&self) -> Option<[f32; 4]> {
        if self.ui.get_place() != "home" {
            return None;
        }
        let i = self.desk.items.iter().position(|it| it.id == "z10")?;
        let [x, y, w, h] = self.desk.screen(i);
        Some([self.ui.get_desk_x() + x, self.ui.get_desk_y() + y, w, h])
    }

    /// The keyboard's size on Home's desk as the view has it, in logical
    /// pixels, whatever shows: for its picture to be drawn ahead of Home's opening.
    pub fn desk_keyboard_size(&self) -> Option<(f32, f32)> {
        let i = self.desk.items.iter().position(|it| it.id == "z10")?;
        let [_, _, w, h] = self.desk.screen(i);
        (self.desk.size.0 > 0.0).then_some((w, h))
    }

    /// Whether the keyboard on the desk is plugged in.
    pub fn desk_keyboard_on(&self) -> bool {
        self.desk.items.iter().find(|it| it.id == "z10").is_none_or(|it| it.connected)
    }

    /// What the desk lays on the window's ground, in the buffer's pixels, while Home shows.
    pub fn desk_decor(&self, scale: f32) -> crate::picture::Decor {
        if self.ui.get_place() != "home" {
            return crate::picture::Decor::default();
        }
        let d = &self.desk;
        // each device's name, under it, for the dots to leave clear
        let names = self.desk_names();
        d.decor((self.ui.get_desk_x(), self.ui.get_desk_y()), scale, &names, &|path| self.pad_photos.get(path).cloned())
    }

    /// Where each device's name is on the desk's room, under it (its words
    /// and, for one not plugged in, the line under them), in logical pixels.
    fn desk_names(&self) -> Vec<[f32; 4]> {
        let d = &self.desk;
        d.items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let [x, y, w, h] = d.screen(i);
                let nw = crate::text::width(500, 13.5, &it.name) + 15.0;
                let sw = if it.connected { 0.0 } else { crate::text::width(400, 12.0, "Not connected") };
                let half = nw.max(sw) / 2.0;
                let top = y + h + crate::desk::NAME_GAP;
                [x + w / 2.0 - half, top, x + w / 2.0 + half, top + if it.connected { 18.0 } else { 38.0 }]
            })
            .collect()
    }

    /// The pads' pictures: each one not read yet, read away from the window's
    /// thread (at once, offscreen), and kept once read for the desk to show;
    /// and the picture asked for a pad, once the desktop's file picker
    /// answers and the file chosen is read, put on the pad.
    pub fn poll_pads(&mut self) {
        let offscreen = crate::desk::offscreen();
        // (a picture no pad shows any more let go of)
        let pads = &self.desk.pads;
        self.pad_photos.retain(|k, _| pads.iter().any(|p| p.picture.as_deref() == Some(k.as_str())));
        let mut wanted: Vec<String> = Vec::new();
        for p in self.desk.pads.iter().filter_map(|p| p.picture.clone()) {
            if !self.pad_photos.contains_key(&p) && !self.pad_unread.contains(&p) && !self.pad_reading.iter().any(|(r, _)| *r == p) && !wanted.contains(&p) {
                wanted.push(p);
            }
        }
        for path in wanted {
            let (tx, rx) = mpsc::channel();
            let p = path.clone();
            let read = move || {
                let _ = tx.send(crate::desk::photo_of(std::path::Path::new(&p)));
            };
            if offscreen {
                read();
            } else {
                std::thread::spawn(read);
            }
            self.pad_reading.push((path, rx));
        }
        let (reading, photos, unread) = (&mut self.pad_reading, &mut self.pad_photos, &mut self.pad_unread);
        reading.retain(|(path, rx)| match rx.try_recv() {
            Ok(Ok(photo)) => {
                photos.insert(path.clone(), std::sync::Arc::new(photo));
                false
            }
            Ok(Err(e)) => {
                eprintln!("ectodeck-native: a pad's picture, {path}: {e}");
                unread.insert(path.clone());
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        let Some((i, was, ask)) = self.pad_asked.take() else { return };
        match ask {
            PadAsk::Picking(rx) => match rx.try_recv() {
                Ok(Some(path)) => {
                    let (tx, rx) = mpsc::channel();
                    let read = move || {
                        let _ = tx.send(crate::desk::import_picture(&path));
                    };
                    if offscreen {
                        read();
                    } else {
                        std::thread::spawn(read);
                    }
                    self.pad_asked = Some((i, was, PadAsk::Reading(rx)));
                    // (read now, offscreen)
                    if offscreen {
                        self.poll_pads();
                    }
                }
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {}
                Err(mpsc::TryRecvError::Empty) => self.pad_asked = Some((i, was, PadAsk::Picking(rx))),
            },
            PadAsk::Reading(rx) => match rx.try_recv() {
                Ok(Ok((path, photo))) => {
                    let key = path.to_string_lossy().to_string();
                    self.pad_unread.remove(&key);
                    self.pad_photos.insert(key.clone(), std::sync::Arc::new(photo));
                    // (a pad deleted or moved while its picture was chosen takes
                    // none, and the copy kept for it goes)
                    if self.desk.set_pad_picture(i, was, Some(key.clone())) {
                        self.pad_note.clear();
                    } else {
                        self.desk.forget_picture(Some(key));
                    }
                    self.show_desk();
                }
                Ok(Err(why)) => {
                    self.pad_note = why;
                    self.show_desk();
                }
                Err(mpsc::TryRecvError::Empty) => self.pad_asked = Some((i, was, PadAsk::Reading(rx))),
                Err(mpsc::TryRecvError::Disconnected) => {}
            },
        }
    }

    /// A picture asked for the chosen pad: the desktop's own file picker
    /// (offscreen, ECTODECK_NATIVE_PAD_PICTURE's file, as if chosen there).
    fn ask_pad_picture(&mut self) {
        let Some((i, b)) = self.desk.chosen_box() else { return };
        if matches!(self.pad_asked, Some((_, _, PadAsk::Picking(_)))) {
            return;
        }
        self.close_picker();
        self.pad_note.clear();
        let rx = match std::env::var_os("ECTODECK_NATIVE_PAD_PICTURE").filter(|_| crate::desk::offscreen()) {
            Some(f) => {
                let (tx, rx) = mpsc::channel();
                let _ = tx.send(Some(std::path::PathBuf::from(f)));
                rx
            }
            None => crate::picker::open_file("Choose a picture for the pad", "Pictures", &["*.png", "*.jpg", "*.jpeg", "*.webp", "*.gif", "*.svg", "*.PNG", "*.JPG", "*.JPEG", "*.WEBP", "*.GIF", "*.SVG"]),
        };
        self.pad_asked = Some((i, b, PadAsk::Picking(rx)));
        // (offscreen, the answer taken at once: a snapshot has no later turn to take it on)
        if crate::desk::offscreen() {
            self.poll_pads();
        }
        self.show_desk();
    }

    /// Delete or Backspace on Home in edit mode: the chosen pad taken off the
    /// desk; false where no pad is chosen (or the color picker is open on it).
    pub fn desk_delete_key(&mut self) -> bool {
        if self.ui.get_place() != "home" || !self.desk.editing || self.desk.pad_chosen.is_none() || self.pick.is_some() {
            return false;
        }
        self.desk.delete_pad();
        self.pad_note.clear();
        self.show_desk();
        true
    }

    /// Ctrl with plus, minus or 0 on Home in edit mode: zoom in, out, or to the
    /// fit (`how` as the zoom controls have it); false anywhere else.
    pub fn desk_zoom_key(&mut self, how: i32) -> bool {
        if self.ui.get_place() != "home" || !self.desk.editing {
            return false;
        }
        self.desk.zoom_step(how);
        self.desk_frames_run();
        true
    }

    /// The mouse's page, as it is now: its tab, its picture, its list and its boxes.
    fn refresh_mouse(&mut self) {
        use crate::mouse::KINDS;
        // (other devices' jobs named as their pages have them now)
        let names = self.job_names();
        if self.mouse.names != names {
            self.mouse.names = names;
        }
        let ui = &self.ui;
        let m = &self.mouse;
        ui.set_mouse_tab(m.tab as i32);
        ui.set_mouse_hint(m.hint().into());
        ui.set_mouse_status(m.failed.clone().unwrap_or_default().into());
        // the profiles, and what was last done to them, with Undo for a moment
        // (a profile tied to apps says so under its name)
        let mut items = m.profile_items();
        if let Some(store) = &m.profiles {
            for (it, p) in items.iter_mut().zip(&store.list) {
                if let Some(w) = self.with_apps(&p.apps) {
                    it.sub = w.into();
                }
            }
        }
        show_rows(ui.get_mouse_profiles(), |r| ui.set_mouse_profiles(r), items);
        ui.set_mouse_profiles_shown(m.profiles.is_some());
        ui.set_mouse_said(m.said_now().into());
        ui.set_mouse_undo_shown(m.undo_shown());
        // (each list kept, its rows changed in place: a list made anew would
        // drop a press the pointer is in the middle of)
        show_rows(ui.get_mouse_calls(), |r| ui.set_mouse_calls(r), m.calls());
        let (lines, dots, on_line, on_dot) = m.lines();
        ui.set_mouse_lines(lines.into());
        ui.set_mouse_dots(dots.into());
        ui.set_mouse_line_on(on_line.into());
        ui.set_mouse_dot_on(on_dot.into());
        show_rows(ui.get_mouse_rows(), |r| ui.set_mouse_rows(r), m.rows());
        ui.set_mouse_chosen(m.button.is_some());
        if let Some(b) = m.button {
            let (cap, title, factory, state, rebound, kind, note) = m.detail(b);
            ui.set_mouse_cap(cap.into());
            ui.set_mouse_title(title.into());
            ui.set_mouse_factory(factory.into());
            ui.set_mouse_state(state.into());
            ui.set_mouse_rebound(rebound);
            ui.set_mouse_kind(kind as i32);
            show_rows(ui.get_mouse_kinds(), |r| ui.set_mouse_kinds(r), KINDS.iter().map(|(n, _)| SharedString::from(*n)).collect());
            show_rows(ui.get_mouse_kind_icons(), |r| ui.set_mouse_kind_icons(r), KINDS.iter().map(|(_, i)| SharedString::from(*i)).collect());
            ui.set_mouse_job(crate::mouse::job_label_with(&m.job(b), &m.names).into());
            ui.set_mouse_note(note.into());
        } else {
            let (note, any) = m.all_buttons();
            ui.set_mouse_all_note(note.into());
            ui.set_mouse_any_rebound(any);
        }
        // the effects' tiles, their pictures the keyboard's own
        if self.mouse_icons.as_ref().map(|(s, _)| *s) != Some(self.scale) {
            let icons = ["off", "static", "breathing", "wave"].iter().map(|e| lighting::icon(e, self.scale)).collect();
            self.mouse_icons = Some((self.scale, icons));
        }
        let fx = self.mouse.effect();
        let effects: Vec<EffectItem> = self
            .mouse
            .effects()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (tile, motion) = crate::mouse::tile_of(e["id"].as_str().unwrap_or(""));
                let icon = self.mouse_icons.as_ref().unwrap().1[["off", "static", "breathing", "wave"].iter().position(|t| *t == tile).unwrap_or(1)].clone();
                EffectItem { name: e["label"].as_str().unwrap_or("").into(), icon, on: i == fx, motion }
            })
            .collect();
        self.ui.set_mouse_effect_moves(effects.iter().any(|e| e.on && e.motion != 0));
        show_rows(self.ui.get_mouse_effects(), |r| self.ui.set_mouse_effects(r), effects);
        // (the Profiles box first in the row, on each tab whose settings belong to a profile)
        let profiles_w = if self.mouse.tab < 3 && self.mouse.profiles.is_some() { 252.0 } else { 0.0 };
        let w = self.ui.get_mouse_w() - if self.mouse.tab == 1 { 564.0 } else { 0.0 } - profiles_w;
        // (the color the picker is open on, ringed)
        let open = self.pick.as_ref().and_then(|p| match p.piece {
            Some(crate::described::Piece::Effect(k)) => Some((k, p.index)),
            _ => None,
        });
        let (cells, h) = self.mouse.cells(w, open);
        self.ui.set_mouse_cells_h(h);
        self.show_cells(cells);
        // the picture: its layers, the buttons it picks out, its lights now, and their effects playing
        self.mouse_draw();
        self.show_mouse_picked();
        self.show_mouse_lights();
        self.mouse_frames_run();
    }

    /// The menu on the mouse's profile in use: Duplicate, and Delete, which
    /// the only profile can't be.
    fn open_profile_menu(&mut self, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        self.close_menu();
        self.close_picker();
        let only = self.mouse.profiles.as_ref().is_none_or(|p| p.list.len() < 2);
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into() };
        let mut rows = vec![row(0, "Duplicate"), row(0, "Use with an app…"), row(2, ""), row(if only { 5 } else { 4 }, "Delete")];
        if only {
            rows.push(row(3, "Your only profile can’t be deleted."));
        }
        self.hang_menu(rows, 220.0, anchor);
        self.profile_menu = true;
    }

    /// A small menu of `rows`, `w` wide, open from `anchor`: under it where
    /// it fits, over it where there's more room there.
    fn hang_menu(&mut self, rows: Vec<MenuRow>, w: f32, anchor: (f32, f32, f32)) {
        let h = 6.0 + rows.iter().map(|r| match r.kind { 1 => 28.0, 2 => 11.0, _ => 33.0 }).sum::<f32>() + 6.0;
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        self.ui.set_menu_w(w);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        let (ax, ay, ah) = anchor;
        let win = self.ui.get_window_height();
        let (below, above) = (win - ay - ah - 14.0, ay - 70.0);
        let down = h <= below || below >= above;
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_ay(ay);
        self.ui.set_menu_ah(ah);
        self.ui.set_menu_top(if down { ay + ah + 6.0 } else { ay - 6.0 - h.min(above) });
        self.ui.set_menu_max(if down { below.max(win * 0.3) } else { above });
        self.ui.set_menu_open(true);
    }

    /// The menu on the keyboard's profile being edited: Use with an app.
    fn open_kb_profile_menu(&mut self, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        self.close_menu();
        self.close_picker();
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into() };
        self.hang_menu(vec![row(0, "Use with an app…")], 220.0, anchor);
        self.kb_profile_menu = true;
    }

    /// The apps a profile can be tied to (the keyboard's, `device` 0, by its
    /// number; the mouse's, 1, by its place): those seen in front, newest
    /// first, and any it's tied to that aren't, each ticked if it is.
    fn open_apps_menu(&mut self, device: usize, profile: usize, anchor: (f32, f32, f32)) {
        self.close_menu();
        let tied = self.tied_apps(device, profile);
        let mut apps: Vec<(String, String)> = self.front_apps.list.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
        for t in &tied {
            if !apps.iter().any(|(id, _)| id == t) {
                apps.push((t.clone(), self.front_apps.name(t)));
            }
        }
        let mut rows = vec![MenuRow { kind: 1, label: "Use it while one of these is in front".into(), tag: "".into(), tick: false, sub: "".into() }];
        rows.extend(apps.iter().map(|(id, name)| MenuRow { kind: 0, label: name.as_str().into(), tag: "".into(), tick: tied.contains(id), sub: "".into() }));
        if apps.is_empty() {
            rows.push(MenuRow { kind: 3, label: "Apps show here once they’ve been in front while Ectodeck is running.".into(), tag: "".into(), tick: false, sub: "".into() });
        }
        self.hang_menu(rows, 300.0, anchor);
        self.apps_menu = Some((device, profile, anchor));
    }

    /// The apps a profile is tied to: the keyboard's (`device` 0, by its
    /// number) as Ectodeck's settings keep them, the mouse's (1, by its place)
    /// with it.
    fn tied_apps(&self, device: usize, profile: usize) -> Vec<String> {
        if device == 1 {
            return self.mouse.profiles.as_ref().and_then(|s| s.list.get(profile)).map(|p| p.apps.clone()).unwrap_or_default();
        }
        self.settings
            .get("devices.z10.profile_apps")
            .and_then(|v| v[profile.to_string()].as_array().cloned())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }

    /// The keyboard's profile tied to an app, if one is.
    fn kb_profile_for(&self, app: &str) -> Option<u8> {
        z10::PROFILE_NUMBERS.iter().copied().find(|&n| self.tied_apps(0, n as usize).iter().any(|a| a == app))
    }

    /// A profile tied to an app, or no longer; an app is tied to one of a
    /// device's profiles at a time, the last it was tied to.
    fn tie_app(&mut self, device: usize, profile: usize, app: &str, on: bool) {
        if device == 1 {
            if let Some(s) = &mut self.mouse.profiles {
                s.tie(profile, app, on);
            }
            return;
        }
        let mut all = self.settings.get("devices.z10.profile_apps").filter(|v| v.is_object()).unwrap_or_else(|| serde_json::json!({}));
        for n in z10::PROFILE_NUMBERS {
            let key = n.to_string();
            let mut list: Vec<String> = all[&key].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            list.retain(|a| a != app);
            if on && n as usize == profile {
                list.push(app.to_string());
            }
            all[&key] = serde_json::json!(list);
        }
        if let Err(e) = self.settings.set("devices.z10.profile_apps", all) {
            self.status(format!("The profile's apps were not saved: {e}"), 2);
        }
    }

    /// The words under a profile's name for the apps it's tied to: "With
    /// Steam", or with how many more.
    fn with_apps(&self, apps: &[String]) -> Option<String> {
        let first = self.front_apps.name(apps.first()?);
        Some(match apps.len() {
            1 => format!("With {first}"),
            n => format!("With {first} and {} more", n - 1),
        })
    }

    /// An app come to the front, as KWin says (see front_app.rs): each device
    /// switches to its profile tied to the app; once an app it followed
    /// leaves the front, back to the profile chosen by hand. Ectodeck's own
    /// windows change nothing, so a profile can be changed with Ectodeck in front.
    pub fn front_app(&mut self, f: crate::front_app::Front) {
        let seen = self.front_apps.saw(&f);
        if f.is_ectodeck() {
            return;
        }
        let app = f.app();
        let mut changed = false;
        // the mouse
        if let Some(store) = &self.mouse.profiles {
            let (active, count) = (store.active, store.list.len());
            match store.for_app(&app) {
                Some(i) if i != active => {
                    self.follow_back[1].get_or_insert(active);
                    self.mouse.choose_profile(i);
                    eprintln!("ectodeck-native: the mouse follows {app}");
                    changed = true;
                }
                Some(_) => {}
                None => {
                    if let Some(back) = self.follow_back[1].take().filter(|b| *b < count && *b != active) {
                        self.mouse.choose_profile(back);
                        changed = true;
                    }
                }
            }
        }
        // the keyboard
        if self.page.dev.is_some() {
            let active = self.page.active();
            match self.kb_profile_for(&app) {
                Some(n) if n != active => {
                    self.follow_back[0].get_or_insert(active as usize);
                    self.choose_profile(n);
                    eprintln!("ectodeck-native: the keyboard follows {app}");
                    changed = true;
                }
                Some(_) => {}
                None => {
                    if let Some(back) = self.follow_back[0].take().filter(|b| *b as u8 != active) {
                        self.choose_profile(back as u8);
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.show_mouse_lights();
        }
        if changed || seen {
            self.refresh();
        }
    }

    /// The chosen button's jobs of one kind, under the Key detail box's menu field.
    fn open_mouse_menu(&mut self, kind: usize, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        self.close_menu();
        self.close_picker();
        let Some(b) = self.mouse.button else { return };
        let now = self.mouse.job(b);
        // (a macro ticked whatever it plays as)
        let same = |a: &serde_json::Value| *a == now || (a.get("macro").is_some() && a["macro"] == now["macro"]);
        let mut rows: Vec<MenuRow> = self.mouse.choices_for(kind).iter().map(|(label, a)| MenuRow { kind: 0, label: label.as_str().into(), tag: "".into(), tick: same(a), sub: "".into() }).collect();
        if rows.is_empty() && kind == crate::mouse::MACRO_KIND {
            rows.push(MenuRow { kind: 3, label: "No macros yet. Make one in the Macro Editor.".into(), tag: "".into(), tick: false, sub: "".into() });
        }
        if rows.is_empty() && kind == crate::mouse::DEVICES_KIND {
            rows.push(MenuRow { kind: 3, label: "Plug in another device to give this button one of its jobs.".into(), tag: "".into(), tick: false, sub: "".into() });
        }
        let h = 6.0 + rows.len() as f32 * 33.0 + 6.0;
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        self.ui.set_menu_w(260.0);
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
        self.ui.set_mouse_menu_open(true);
        self.mouse.menu = Some(kind);
    }

    /// The Lighting page's effects and its tile, laid out for the draft. The
    /// pieces are changed in place, so one being used is never rebuilt under
    /// the pointer.
    fn refresh_lighting(&mut self) {
        let mode = self.page.draft()["lighting"]["keys"]["effect"].as_str().unwrap_or("off").to_string();
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
        let l = &d["lighting"]["keys"];
        let mode = l["effect"].as_str().unwrap_or("off");
        p.view == View::Lighting && mode != "off" && mode != "custom" && l["effects"][mode]["zone"].as_u64().unwrap_or(0x80) & 0x80 == 0
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

    /// "New screen layout" selected, at the pointer: From scratch, a copy of a layout, or a
    /// ready-made one put back.
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

    /// Layout `i` moved to place `to` in the Library; the editor's window stays on the layout
    /// it is open on, and each card keeps its picture.
    fn move_layout(&mut self, i: usize, to: usize) {
        let Some(to) = self.screens.move_page(i, to) else { return };
        if let Some(ed) = self.editor.as_mut() {
            ed.page = match ed.page {
                p if p == i => to,
                p if i < p && to >= p => p - 1,
                p if i > p && to <= p => p + 1,
                p => p,
            };
        }
        if i < self.pl_thumbs.len() {
            let pic = self.pl_thumbs.remove(i);
            self.pl_thumbs.insert(to.min(self.pl_thumbs.len()), pic);
        }
        self.pages_edited();
        self.refresh_editor();
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

    /// Where the readings that follow a choice come from now, for the editor's From line:
    /// the main card's whole name, and the driver's name for the processor's own temperature.
    fn reading_from(&mut self) -> crate::screen_edit::From {
        use crate::lcd_page::Kind;
        let src = self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)?.pieces.get(ed.chosen?)).and_then(|p| match p.kind {
            Kind::Data(s, _) | Kind::Bar(s) => Some(s),
            _ => None,
        });
        let Some(src) = src else { return Default::default() };
        let setup = crate::sensors::setup();
        let cat = self.reader.get_or_insert_with(crate::lcd_stats::Stats::here).catalog();
        crate::screen_edit::From { line: crate::screen_edit::from_line(src, cat, &setup) }
    }

    /// The computer's readings a block can show, for the Shows menu: the sensors checked,
    /// under the devices they read.
    fn shows_groups(&mut self) -> Vec<(String, Vec<(String, crate::lcd_page::Source)>)> {
        let setup = crate::sensors::setup();
        let cat = self.reader.get_or_insert_with(crate::lcd_stats::Stats::here).catalog();
        crate::screen_edit::computer_groups(cat, &setup)
    }

    /// Settings, at its Sensors box (from the screen editor's From line, or its Shows menu).
    pub fn show_sensors(&mut self) {
        self.ui.set_place("settings".into());
        self.refresh();
        let to = (self.sensor_map.top - 24.0).max(0.0);
        self.ui.set_settings_goto(-1.0);
        self.ui.set_settings_goto(to);
    }

    /// What the Sensors box shows: the computer's sensors and their readings, read again
    /// when a second old; the choices; the tab open, the runs opened, the sensor renamed. A
    /// card that has come in as the main card has its usual sensors checked as it is seen.
    fn sensors_view(&mut self) -> crate::ecto_settings::Sensors {
        if self.sensors_seen.as_ref().is_none_or(|(t, _, _)| t.elapsed() >= Duration::from_secs(1)) {
            let stats = self.reader.get_or_insert_with(crate::lcd_stats::Stats::here);
            let values = stats.read_all();
            let cat = stats.catalog().clone();
            let mut setup = crate::sensors::setup();
            if setup.settle(&cat) {
                self.keep_setup(setup);
            }
            self.sensors_seen = Some((Instant::now(), cat, values));
        }
        let (_, cat, values) = self.sensors_seen.clone().unwrap();
        crate::ecto_settings::Sensors { cat, setup: crate::sensors::setup(), values, tab: self.sensor_tab, open: self.sensor_open.clone(), renaming: self.renaming.clone() }
    }

    /// The sensors' choices changed: kept in the settings, and gone by at once by every
    /// reader of the screen's readings; the editor's and the playlist's pictures read again.
    fn keep_setup(&mut self, s: crate::sensors::Setup) {
        if let Err(e) = self.settings.set("sensors", s.to_json()) {
            self.status(format!("The sensors were not saved: {e}"), 2);
            return;
        }
        if let Some((_, cat, _)) = &self.sensors_seen {
            crate::sensors::remember(cat, &s);
        }
        crate::sensors::set_up(s);
        self.read_at = None;
    }

    /// The layouts that show a sensor, by name: one shown by its number, and the processor's,
    /// the memory's and the main card's usual readings shown as the ready-made layouts show them.
    fn shown_in(&self, id: &str, cat: &crate::sensor_list::Catalog) -> Vec<String> {
        use crate::lcd_page::{Kind, Source};
        let main = crate::sensors::setup().main_card(cat).map(|d| d.key.clone()).unwrap_or_default();
        let reads = |s: Source| -> bool {
            match s {
                Source::Sensor(n) => crate::sensors::id_of(n) == id,
                Source::CpuTemp => cat.cpu_temp().is_some_and(|t| t.id == id),
                Source::CpuLoad => id == "cpu/load",
                Source::CpuClock => id == "cpu/clock/average",
                Source::Memory => id == "memory/used",
                Source::GpuTemp => id == format!("{main}/temperature"),
                Source::GpuLoad => id == format!("{main}/load"),
                Source::GpuClock => id == format!("{main}/clock"),
                Source::VideoMemory => id == format!("{main}/video-memory"),
                _ => false,
            }
        };
        self.screens.library.iter().filter(|p| p.pieces.iter().any(|q| matches!(q.kind, Kind::Data(s, _) | Kind::Bar(s) if reads(s)))).map(|p| p.name.clone()).collect()
    }

    /// A sensor's checkbox clicked: checked; or unchecked, asked about first where a layout
    /// shows it, in the app's own dialog.
    fn toggle_sensor(&mut self, id: &str) {
        let Some((_, cat, _)) = self.sensors_seen.clone() else { return };
        let mut s = crate::sensors::setup();
        let on = s.checked_in(&cat).contains(id);
        let shows = if on { self.shown_in(id, &cat) } else { Vec::new() };
        if shows.is_empty() {
            s.check(&cat, id, !on);
            self.keep_setup(s);
            return;
        }
        let what = crate::screen_edit::sensor_phrase(id, &cat, &s);
        let them = if shows.len() == 1 { "it" } else { "them" };
        let show = if shows.len() == 1 { "shows" } else { "show" };
        self.ui.set_confirm_title(format!("Uncheck {what}?").into());
        self.ui.set_confirm_body(format!("{} {show} {what}. Unchecked, it shows as unavailable in {them}.", crate::sensors::and_list(&shows)).into());
        self.ui.set_confirm_keep("Keep".into());
        self.ui.set_confirm_do("Uncheck".into());
        self.unchecking = Some(id.to_string());
        self.ui.set_confirm_open(true);
    }

    /// The Sensors card's choices carried over into the sensors checked: the sensor and the
    /// clock it chose are checked, its card is the main card, and a layout showing the CPU
    /// temperature or clock from other than the usual sensor shows that sensor, as it did.
    fn carry_sensors(&mut self, b: &crate::sensors::Before) {
        use crate::lcd_page::{Kind, Source};
        // (the processor's and the motherboard's sensors are Linux's own files; NVIDIA's driver isn't needed)
        let cat = crate::sensor_list::catalog(std::path::Path::new("/"), &mut crate::sensor_list::NoNvidia);
        let mut setup = crate::sensors::setup();
        let shows = setup.carry(b, &cat);
        let mut moved = false;
        for page in &mut self.screens.library {
            for p in &mut page.pieces {
                if let Kind::Data(s, _) | Kind::Bar(s) = &mut p.kind {
                    if let Some((_, id)) = shows.iter().find(|(from, _)| from == s) {
                        *s = Source::Sensor(crate::sensors::number(id));
                        moved = true;
                    }
                }
            }
        }
        if moved {
            if let Err(e) = self.settings.set("devices.z10.screen.pages", self.screens.to_json()["pages"].clone()) {
                eprintln!("ectodeck-native: the screen layouts were not saved with their sensors: {e}");
            }
        }
        if let Err(e) = self.settings.set("sensors", setup.to_json()) {
            eprintln!("ectodeck-native: the sensors were not saved: {e}");
        }
        crate::sensors::set_up(setup);
    }

    /// Layout `k`'s Delete chosen from its card's menu: first the question, over the window,
    /// saying what goes with the layout.
    fn ask_delete(&mut self, k: usize) {
        let Some(page) = self.screens.library.get(k) else { return };
        let says = self.screens.delete_says(k, self.onboard_ready(), |n| self.page.profile_name(n));
        self.ui.set_confirm_title(format!("Delete {}?", page.name).into());
        self.ui.set_confirm_body(says.into());
        self.ui.set_confirm_keep("Cancel".into());
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
        let mut lights = self.page.lights(t);
        // (the desk's light, while it plays, on each key it lights)
        if self.desk_light != crate::desk_light::Effect::Off {
            for l in &mut lights {
                if let Some(c) = self.desk_colors.0.get(&l.pos) {
                    (l.rgb, l.strength) = (*c, 1.0);
                }
            }
        }
        if self.last_lights.as_ref() == Some(&lights) {
            return None;
        }
        self.last_lights = Some(lights.clone());
        Some(lights)
    }

    pub fn animated(&self) -> bool {
        self.page.animated() || self.desk_light != crate::desk_light::Effect::Off
    }

    /// Home's Desk light menu: its effects, the one playing ticked.
    fn open_desk_light_menu(&mut self, anchor: (f32, f32, f32)) {
        if let Some((at, when)) = self.shut.take() {
            if (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800) {
                return;
            }
        }
        self.close_menu();
        let rows = crate::desk_light::EFFECTS.iter().map(|(e, _, w)| MenuRow { kind: 0, label: (*w).into(), tag: "".into(), tick: *e == self.desk_light, sub: "".into() }).collect();
        self.hang_menu(rows, 240.0, anchor);
        self.desk_light_menu = true;
    }

    /// The desk's light set to an effect, kept so: its frames from now, or,
    /// off, each device's own lighting handed back to it.
    pub fn set_desk_light(&mut self, e: crate::desk_light::Effect) {
        use crate::desk_light::Effect;
        let was = self.desk_light;
        self.desk_light = e;
        if let Err(err) = self.settings.set("desk.light", serde_json::json!(e.id())) {
            self.status(format!("The desk's light was not saved: {err}"), 2);
        }
        if e == Effect::Off {
            self.desk_light_timer = None;
            if was != Effect::Off {
                self.desk_hand_back();
            }
        } else if self.desk_light_timer.is_none() {
            self.desk_light_t0 = Instant::now();
            let t = slint::Timer::default();
            let asks = self.asks.clone();
            // (the keyboard takes 25 frames a second)
            t.start(slint::TimerMode::Repeated, Duration::from_millis(40), move || asks.borrow_mut().push(Ask::DeskLightFrame));
            self.desk_light_timer = Some(t);
        }
        self.ui.set_desk_light(e.short().into());
        self.show_mouse_lights();
    }

    /// Each device's own lighting again, the desk's light over: the keyboard
    /// out of its direct colors, the mouse's lights as it keeps them.
    fn desk_hand_back(&mut self) {
        self.ask_kb(device::Ask::Preview(serde_json::json!({ "stop": true, "tab": "native", "claim": true })));
        self.mouse.hand_back();
        self.desk_colors = Default::default();
        self.desk_light_mouse = (None, [[0; 3]; 2]);
        self.last_lights = None;
    }

    /// The desk's light off for Ectodeck quitting, each device its own again;
    /// kept as it was, to play again next time.
    pub fn desk_light_quit(&mut self) {
        if self.desk_light != crate::desk_light::Effect::Off {
            self.desk_light_timer = None;
            self.desk_hand_back();
        }
    }

    /// Where each device's lights are on Home's desk, in millimeters: the
    /// keyboard's keys with LEDs, the mouse's logo and wheel.
    fn desk_places(&self) -> crate::desk_light::Places {
        let mut out = crate::desk_light::Places::default();
        let item = |id: &str| self.desk.items.iter().find(|it| it.id == id).map(|it| [it.x, it.y, it.w, it.h]);
        if let (Some(r), true) = (item("z10"), self.page.dev.is_some()) {
            let vb = self.page.view_box;
            out.keys = self.page.layout.iter().filter(|k| k.led).map(|k| {
                let (x, y) = crate::desk_light::on_desk(vb, r, k.cx, k.cy);
                (k.pos, x, y)
            }).collect();
        }
        if let Some(r) = item("mouse") {
            let v = crate::mouse_art::VIEW;
            let view = (v.0 as f64, v.1 as f64, v.2 as f64, v.3 as f64);
            let middles = match &self.mouse_layers {
                Some((scale, imgs)) => crate::mouse_art::light_boxes(&imgs.layers, (240.0 * scale).round() as u32).map(|b| ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)),
                None => crate::mouse_art::LIGHT_MIDDLES,
            };
            out.mouse = crate::mouse_art::LIGHT_IDS.iter().zip(middles).map(|(id, (x, y))| {
                let (dx, dy) = crate::desk_light::on_desk(view, r, x as f64, y as f64);
                (*id, dx, dy)
            }).collect();
        }
        out
    }

    /// The desk's light's next frame: each light's color where it sits, sent
    /// to its device (the keyboard every frame, but while its Lighting tab
    /// previews; the mouse as often as it takes two commands, when its colors
    /// change), and the pictures with it.
    fn desk_light_frame(&mut self) {
        let e = self.desk_light;
        if e == crate::desk_light::Effect::Off {
            return;
        }
        let t = self.desk_light_t0.elapsed().as_secs_f32();
        let places = self.desk_places();
        let keys: HashMap<u32, [u8; 3]> = places.keys.iter().map(|(pos, x, y)| (*pos, crate::desk_light::color(e, *x, *y, t))).collect();
        let mouse = [0, 1].map(|i| places.mouse.iter().find(|(id, ..)| *id == crate::mouse_art::LIGHT_IDS[i]).map(|(_, x, y)| crate::desk_light::color(e, *x, *y, t)));
        self.desk_colors = (keys, mouse);
        let previewing = self.ui.get_place() == "z10" && self.page.view == View::Lighting;
        if self.page.dev.is_some() && !previewing {
            let colors: Vec<[u8; 3]> = (0..ectotype_core::protocol::NKEYS as u32).map(|p| self.desk_colors.0.get(&p).copied().unwrap_or([0; 3])).collect();
            self.ask_kb(device::Ask::Preview(serde_json::json!({ "colors": colors, "tab": "native", "claim": true })));
        }
        if let (true, Some(logo), Some(wheel)) = (self.mouse.present(), mouse[0], mouse[1]) {
            let due = self.desk_light_mouse.0.is_none_or(|at| at.elapsed() >= Duration::from_millis(125));
            if due && self.desk_light_mouse.1 != [logo, wheel] {
                self.mouse.direct(serde_json::json!({ "logo": lighting::hex_of(logo), "wheel": lighting::hex_of(wheel) }));
                self.desk_light_mouse = (Some(Instant::now()), [logo, wheel]);
            }
        }
        if self.mouse_pictured() {
            self.show_mouse_lights();
        }
    }

    /// The desk's light's frame now, as its timer would give it (a snapshot
    /// has no timers).
    pub fn desk_light_now(&mut self) {
        self.desk_light_frame();
    }

    /// A mouse light's color for its picture now: the desk's light's, while
    /// it plays; else the mouse's own, none while that light is off.
    fn mouse_color(&self, i: usize, now: Instant) -> Option<[u8; 3]> {
        if self.desk_light != crate::desk_light::Effect::Off {
            if let Some(c) = self.desk_colors.1[i] {
                return Some(c);
            }
        }
        self.mouse.lit(crate::mouse::LIGHTS[i]).map(|_| self.mouse.color_at(i, now))
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

    /// The layouts Ectodeck ships that the Library was never given, put at its end once and
    /// saved, with the list of those given, so a layout deleted after stays deleted (and a
    /// layout Ectodeck ships later is given once too).
    fn give_built_ins(&mut self) {
        let given = self.settings.get("devices.z10.screen.built_ins");
        let all = serde_json::json!(crate::lcd_page::BUILT_INS);
        if given.as_ref() == Some(&all) {
            return;
        }
        let added = self.screens.give_built_ins(given.as_ref());
        // (the layouts first, so the list never names layouts the Library wasn't saved with)
        let saved = if added > 0 { self.settings.set("devices.z10.screen.pages", self.screens.to_json()["pages"].clone()) } else { Ok(()) };
        if let Err(e) = saved.and_then(|_| self.settings.set("devices.z10.screen.built_ins", all)) {
            eprintln!("ectodeck-native: the new screen layouts were not saved: {e}");
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
            s.prime();
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
        // the picture of the keyboard shows each frame as the keyboard gets it (it read the
        // frames' file every two seconds, so it ran up to two seconds behind, Grant saw)
        if self.player_drives() {
            let rows = self.lcd.rows.clone();
            self.show_lcd(true, &rows);
        }
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
        let (onboard, profiles, from, groups) = (self.onboard_ready(), crate::lcd_page::PROFILES.map(|n| self.page.profile_name(n)), self.reading_from(), self.shows_groups());
        let Some(l) = &mut self.layouts else { return Vec::new() };
        let mut cx = crate::layout_page::Cx { screens: &mut self.screens, editor: &mut self.editor, readings: &self.readings, avail: &avail, light, t, onboard, profiles, from, groups };
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
        let (onboard, profiles, from, groups) = (self.onboard_ready(), crate::lcd_page::PROFILES.map(|n| self.page.profile_name(n)), self.reading_from(), self.shows_groups());
        let Some(l) = &mut self.layouts else { return };
        let cx = crate::layout_page::Cx { screens: &mut self.screens, editor: &mut self.editor, readings: &self.readings, avail: &avail, light, t, onboard, profiles, from, groups };
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
                time: if n > 1 && !e.stays { format!("{} s", e.secs).into() } else { "".into() },
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
        // (a layout carried along the Library: the others make room where it will land)
        let along = self.carry.as_ref().filter(|c| !c.from_strip).and_then(|c| Some((c.index, c.shelf?)));
        let places = shelf_places(self.screens.library.len(), along);
        self.ui.set_lib_gap(along.map_or(-1, |(_, to)| to as i32));
        let mut cards = Vec::with_capacity(self.screens.library.len());
        for (k, p) in self.screens.library.iter().enumerate() {
            let rgb = crate::z10::as_lit(p.light.unwrap_or(profile));
            let key: PicKey = (crate::lcd_page::render_still(p, &self.readings), rgb, level, px(260.0));
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
                at: places[k] as i32,
                carried: along.is_some_and(|(from, _)| from == k),
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
                    crate::lcd_page::status_rows(d["lighting"]["screen"]["apps"].as_u64().unwrap_or(0), self.page.editing, d["name"].as_str().unwrap_or(""), None)
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
    /// carried from the Library, while it is over the Library: the place there it lands in
    shelf: Option<usize>,
}

/// The place each of the Library's `n` cards shows in: its own, or, while the layout at
/// `from` is carried along the Library to land at `to` (`along`), the others moved out of
/// the way so the place it will land in is free (the carried one keeps its own place, left
/// out of the picture).
fn shelf_places(n: usize, along: Option<(usize, usize)>) -> Vec<usize> {
    (0..n)
        .map(|k| match along {
            Some((from, to)) if k != from => {
                let without = if k < from { k } else { k - 1 };
                if without >= to { without + 1 } else { without }
            }
            _ => k,
        })
        .collect()
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


/// The colors used lately, newest first, as the picker lists them.
pub(crate) fn recent_colors() -> Vec<[u8; 3]> {
    crate::uistate::get("recent_colors").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub(crate) fn keep_recent(c: [u8; 3]) {
    let mut list = vec![c];
    list.extend(recent_colors().into_iter().filter(|r| *r != c));
    list.truncate(9);
    crate::uistate::set("recent_colors", serde_json::json!(list));
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
    /// How much brighter than the page's colors the keys' light is drawn:
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
    let c = &l["screen"]["color"];
    let b = |v: &serde_json::Value| v.as_u64().unwrap_or(0).min(255) as u8;
    ([c["brightness"].as_u64().map_or(255, |v| v.min(255) as u8), b(&c["color"][0]), b(&c["color"][1]), b(&c["color"][2])], b(&l["screen"]["apps"]))
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

    #[test]
    fn new_screen_layout_offers_each_deleted_ready_made_layout_back() {
        let mut s = crate::lcd_page::Screens::of(None, None, None, None);
        let labels = |rows: &[MenuRow]| rows.iter().map(|r| (r.kind, r.label.to_string())).collect::<Vec<_>>();
        // every layout Ectodeck ships in the Library: From scratch, then a copy of any of them
        let (rows, made) = new_rows(&s.library);
        assert_eq!(rows.len(), 3 + crate::lcd_page::BUILT_INS.len());
        assert!(!labels(&rows).contains(&(1, "Add a ready-made layout".to_string())));
        assert_eq!(made[3], Some(NewFrom::Copy("stats".into())));
        // Rain and Music deleted: both offered back under their own heading, as named
        for id in ["rain", "music"] {
            s.delete_page(s.index_of(id).unwrap());
        }
        let (rows, made) = new_rows(&s.library);
        let n = rows.len();
        assert_eq!(labels(&rows)[n - 4..], [(2, String::new()), (1, "Add a ready-made layout".to_string()), (0, "Rain".to_string()), (0, "Music".to_string())]);
        assert_eq!(made[n - 4..], [None, None, Some(NewFrom::Ready("rain".into())), Some(NewFrom::Ready("music".into()))]);
    }

    #[test]
    fn each_ready_made_layout_says_what_it_shows_in_two_lines_of_its_card() {
        // (a card's words in two columns of cards at the window's first size, 1440 wide: about
        // 159 px, so a little less)
        for p in crate::lcd_page::Screens::of(None, None, None, None).library {
            let (a, b) = two_lines(&p.about(), 155.0);
            assert!(!b.ends_with('…'), "{}: {a} / {b}", p.name);
        }
    }

    #[test]
    fn a_layout_carried_along_the_library_has_the_others_make_room() {
        assert_eq!(shelf_places(4, None), [0, 1, 2, 3]);
        // the second to the end: the third and fourth move up a place, the end left free
        assert_eq!(shelf_places(4, Some((1, 3))), [0, 1, 1, 2]);
        // the last to the start: the rest move down a place
        assert_eq!(shelf_places(4, Some((3, 0))), [1, 2, 3, 3]);
        // over its own place: nothing moves
        assert_eq!(shelf_places(4, Some((2, 2))), [0, 1, 2, 3]);
        // and every place but the one it lands in is taken once
        for from in 0..5 {
            for to in 0..5 {
                let mut taken: Vec<usize> = shelf_places(5, Some((from, to))).into_iter().enumerate().filter(|(k, _)| *k != from).map(|(_, p)| p).collect();
                taken.sort();
                assert_eq!(taken, (0..5).filter(|p| *p != to).collect::<Vec<_>>(), "{from} to {to}");
            }
        }
    }
}
