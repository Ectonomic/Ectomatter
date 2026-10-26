//! The Z10's page as the window shows it: what the page asks for, what the
//! keyboard answers, and the models the page is drawn from. Nothing here
//! knows about the window, so the offscreen check drives it the same way.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ectomatter_draw::hdr::Light;
use slint::{Color, Model, ModelRc, SharedString, VecModel};

use crate::device;
use crate::lighting::{self, Ctl};
use crate::overlays::Overlays;
use crate::z10::{self, Entry, Page, View};
use ectomatter_protocol::Action;
use crate::{AppWindow, Cell, CheckItem, DeskItem, DeskPad, EffectItem, KeyItem, LayerItem, LayerTab, LibGroup, LibRow, MenuRow, MousePatch, ProfileItem};

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
/// with this id, or the layout Ectomatter ships with this key, as it shipped.
#[derive(Clone, Debug, PartialEq)]
pub enum NewFrom {
    Scratch,
    Copy(String),
    Ready(String),
}

/// The New screen layout menu, in either window: From scratch, then under a heading every
/// layout there is, to start from a copy of it, then under another each layout Ectomatter
/// ships that was deleted, to put back as it shipped; and what each row makes (None for the
/// rules and the headings).
pub fn new_rows(library: &[crate::lcd_page::Page]) -> (Vec<MenuRow>, Vec<Option<NewFrom>>) {
    let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
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
    /// the Library (games_page.rs): a tab, what is typed in its find box, the
    /// store's or the order's menu (where it hangs), a cover or an app's row
    /// opened or its star, Every game's page, Add a game or an app, back to
    /// the Library, the page's star and Play; in the form, Choose…, Add, the
    /// form shut, an app chosen in it
    GlTab(i32),
    GlFind(String),
    GlMenu(crate::games_page::Menu, f32, f32, f32, f32),
    GlOpen(i32),
    GlStar(i32),
    GlEvery,
    GlAdd,
    GlBack,
    GlStarPage,
    GlPlay,
    GlProgram,
    GlAddDone,
    GlClose,
    GlSeen(i32),
    GlRemove,
    GlUndo,
    /// a page's On this page entry selected; in a script's form, its two
    /// checkboxes, its time limit's menu (where it hangs), Remove, and words typed
    GlToc(i32),
    GlScriptWait,
    GlScriptKeep,
    GlScriptLimit(f32, f32, f32, f32),
    GlScriptRemove,
    GlScriptTyped,
    /// a variable's form: Remove, and a word typed (Add turns on for a name)
    GlVarRemove,
    GlVarTyped,
    /// in Add a plugin: Choose…, and an address typed
    GlPluginFile,
    GlPluginTyped,
    /// the Screen tab's Game launch: a row's checkbox, its layout's bar selected, its
    /// until menu (where it hangs), and a layout carried over its rows (which row, or
    /// -1, and its place in Layouts)
    GlRowOn(i32),
    GlRowPick(i32),
    GlRowUntil(i32, f32, f32, f32, f32),
    GlCarryMove(i32, i32),
    /// the mouse's page: a tab, a button or light from its list or its picture,
    /// a kind of job (and where its switch is), the job's menu opened, the
    /// button back to its factory job, an effect; the pointer on the picture
    /// (where, and whether over it) and a click there, the pointer on a
    /// button's words, and the next frame of the lights' effects
    MouseTab(i32),
    MouseRow(i32),
    /// a box the page's row of boxes laid out came out a new width: its pieces
    /// laid out anew to it (the Lighting tab's, the mouse's), and the effects'
    /// pictures drawn anew to their tiles
    BoxesResized,
    /// the mouse's Key library on Buttons: a group's heading pressed (it opens
    /// or closes), and what is typed in its find box
    MouseLibToggle(String),
    MouseLibFind(String),
    MouseKind(i32, f32, f32, f32),
    MouseJob(f32, f32, f32, f32),
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
    /// the Macro Editor, from the mouse's bar: on Ectomatter's own macros
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
    /// Add pad's menu, where its button is; the chosen pad's Size menu; Draw
    /// your own put away unused
    DeskAddPad(f32, f32, f32),
    DeskPadSize(f32, f32, f32),
    DeskDrawCancel,
    /// scenes: Home's Scenes menu, where its button is; the next of the scene's
    /// frames; a scene's pencil in a menu; the scene form's effect tiles, its
    /// Cancel (or Escape, or a press off it), its Create or Save, its Delete
    SceneMenu(f32, f32, f32),
    SceneFrame,
    MenuEdit(i32),
    SceneTile(i32),
    SceneFormClose,
    /// Escape on the scene form: its open dropdown or the picker first
    SceneFormEscape,
    SceneFormDone,
    SceneFormDelete,
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
    ActionMenu(i32, f32, f32, f32, f32),
    Listen(i32),
    Reset,
    MenuChose(i32),
    /// a field in the open menu typed in and set, by its row
    MenuField(i32, String),
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
    CellPick(i32, i32, f32, f32, f32, f32),
    CellLeft(i32, i32, bool),
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
    CellTyped(i32, i32, String),
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
    /// the Screen tab's other state opened: while Ectomatter runs, or when it is closed
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
/// it, then the file chosen being read (and kept in Ectomatter's data).
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
    /// how one of Ectomatter's macros plays, set in the Macro Editor, for the
    /// controls that run it
    MacroPlays(u32, ectomatter_protocol::Plays),
    /// one of Ectomatter's macros deleted: the controls that ran it do nothing now
    MacroGone(u32),
    /// one of Ectomatter's macros saved with new steps: the controls that run it
    /// are told again (a mouse that holds it itself, the macro as it is now)
    MacroSaved(u32),
    /// open the lab's window
    OpenLab,
    /// open the screen layout editor's window, on this layout of the Library
    OpenLayouts(usize),
    /// bring the main window forward on Ectomatter's Settings (from the screen editor's
    /// From line, which links to where its sensor is chosen)
    ShowSettings,
}

/// What a row of Home's Scenes menu does: No scene, a scene turned on (or
/// off, the one on), New scene…; a line between groups does nothing.
#[derive(Clone, Debug, PartialEq)]
enum SceneRow {
    Off,
    Scene(String),
    New,
    Line,
}

/// The scene form, open over the page: the scene it changes (none for a new
/// one), and what it holds so far: the name, whether one effect lights the
/// whole desk, that effect, each device's part (the keyboard's and the
/// mouse's, by their keys) and each plugin device's profile.
struct SceneForm {
    editing: Option<String>,
    name: String,
    desk: bool,
    look: crate::scenes::Look,
    parts: Vec<(String, crate::scenes::Part)>,
    plugins: Vec<(String, String)>,
}

impl SceneForm {
    /// The scene as the form has it now: to play while the form is open, and to keep.
    fn scene(&self) -> crate::scenes::Scene {
        use crate::scenes::{Lights, Scene};
        let lights = if self.desk { Lights::Desk(self.look.clone()) } else { Lights::Each(self.parts.clone()) };
        Scene { id: self.editing.clone().unwrap_or_default(), name: self.name.clone(), lights, plugins: self.plugins.clone(), built_in: false }
    }
}

/// The scene form's dropdown open: a device's effect (by its key), or a
/// plugin device's profile (by its id).
#[derive(Clone, Debug, PartialEq)]
enum FormMenu {
    Part(String),
    Plugin(String),
}

/// The pads' menus on Home: Add pad's, and the chosen pad's Size.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PadMenu {
    Add,
    Size,
}

/// What a row of a pad menu stands for: one of the makers' sizes, Draw your
/// own, or the chosen pad's width or depth to type.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PadRow {
    Size(usize),
    Draw,
    Width,
    Depth,
}

const LAYERS: [(&str, &str); 2] = [("base", "When pressed"), ("fn", "When Fn is held")];

/// The action menu's pages under "Other": each one's title and its list.
const PAGES: [&str; 7] = ["A key", "A media key", "A mouse button", "Switch profile", "Switch screen layout", "Switch playlist", "Keyboard function"];
/// the pages of profiles and of the screen's jobs among them, and the page of macros,
/// opened from the key's Macro switch (other devices' profiles, and Ectomatter's own
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
    /// one of Ectomatter's own jobs, by its id (see jobs.rs; its code found when it is given)
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

/// What the record of changes calls Custom Scripts', Ectomatter's own section of
/// the person's commands.
/// The start plans made so far, each numbered, so a game's start answers only
/// the launcher waiting on that plan, never one from before it.
static PLANS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
pub const CUSTOM_SCRIPTS: &str = "custom-scripts";

/// The form for adding a plugin, while it's open: the repository or file it's
/// to come from, what was found there (its manifest) or why nothing was, and
/// a look, a file being chosen, or the plugin being put in place, while one is.
pub struct PackageForm {
    /// what it's for: a core (Settings' Add a core) or a plugin
    kind: crate::packages::Kind,
    repo: Option<String>,
    file: Option<std::path::PathBuf>,
    found: Option<serde_json::Value>,
    why: Option<String>,
    typed_at: Option<Instant>,
    looked_for: Option<String>,
    looking: Option<mpsc::Receiver<Result<serde_json::Value, String>>>,
    picking: Option<mpsc::Receiver<Option<std::path::PathBuf>>>,
    adding: Option<mpsc::Receiver<Result<crate::core_host::Manifest, String>>>,
}

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
    /// the mouse's Key library on Buttons, as the keyboard's: the groups
    /// closed (none until it is first shown, then all of them where the whole
    /// list runs past 600 high, as the keyboard's 116 keys do), what is
    /// searched for, the button whose group was last opened to show it, and
    /// each group's rows, kept to change in place
    mouse_lib_shut: Option<std::collections::HashSet<String>>,
    mouse_lib_query: String,
    mouse_lib_revealed: Option<usize>,
    mouse_lib_rows: HashMap<String, Rc<VecModel<LibRow>>>,
    /// the screen's light and built-in screens the keyboard was last sent (or
    /// had when it was read), so a change to them shows on it at once
    screen_sent: Option<([u8; 4], u8)>,
    slider_sel: usize,
    /// "Press a key": the layer listening, and the modifiers held so far
    pub listening: Option<(usize, u32)>,
    /// a menu closed by a press on its own button, which that button's click must not open again
    shut: Option<((f32, f32), Instant)>,
    t0: Instant,
    /// the moment the keys' light is drawn at, in milliseconds, for a snapshot
    /// to be the same every time (ECTOMATTER_LIGHT_AT)
    pub light_at: Option<f64>,
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
    /// id, as Ectomatter's settings keep it
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
    /// the effects' tiles' pictures, for the scale and the effects they were drawn for
    icons: Option<(f32, f32, Vec<String>, Vec<slint::Image>)>,
    cells: Rc<VecModel<Cell>>,
    /// the scene form's pieces, under its preview
    scene_cells: Rc<VecModel<Cell>>,
    /// the Where and when box's pieces; the zones chosen for each profile's
    /// effects, kept while the whole keyboard is lit; the setting whose menu
    /// is open (by its place in the effect's settings); the zone the pointer
    /// is over, which the keyboard shows
    /// the Lighting tab's boxes beside the effects, and the mouse's, laid out here
    light_boxes: crate::boxes::Row,
    mouse_boxes: crate::boxes::Row,
    areas: HashMap<(u8, String), u64>,
    light_menu: Option<usize>,
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
    /// the Library, its page, and the games running (games_page.rs); whether
    /// each device's ties made before the Library kept them have been taken in
    /// (the keyboard's, the mouse's)
    pub games: crate::games_page::Games,
    ties_taken: [bool; 2],
    /// the cores and plugins Ectomatter runs itself (hosted.rs), and the thread that makes the plugins' changes
    /// as a game starts and puts them back as it quits (putback.rs); whether what
    /// an Ectomatter before this one left changed has been settled; the profile each
    /// device a plugin brought in was switched to for the app in front
    pub hosted: crate::hosted::Hosted,
    pub worker: crate::putback::Worker,
    settled: bool,
    following: HashMap<String, String>,
    /// the packages, in Settings' order, as their glyphs are given to the window;
    /// the form for adding one, while it's open; the daily look for updates
    packages: Vec<crate::packages::Package>,
    package_form: Option<PackageForm>,
    updates: crate::updates::Checker,
    /// each window open, by KWin's id for it, and the app it's from; the apps not
    /// run through Steam whose plugins' changes are made, from their first window
    /// opening to their last closing
    windows: HashMap<String, String>,
    app_sessions: std::collections::HashSet<String>,
    /// games started through Ectomatter (launcher.rs): what's known of this
    /// computer for them, read on a thread of its own, and when it was last
    /// asked for; the choices and what's known the recipes were last written
    /// for; whether the launcher is installed in Steam; each game whose start
    /// changes are being made (true once made), the launchers waiting on
    /// them, and the starts made before the Library's watch saw their game;
    /// the jobs running (Steam's settings changed, a prefix backed up), each
    /// to say how it went, and whether Steam's are among them
    facts_reader: Option<crate::launcher::Reader>,
    known: Option<crate::launcher::Known>,
    known_asked: Option<Instant>,
    recipes_for: Option<(String, crate::launcher::Known)>,
    tool_installed: bool,
    starting: HashMap<String, (u64, bool)>,
    holds: Vec<(String, u64, mpsc::Sender<()>, Instant)>,
    early: Vec<(String, u32, Instant)>,
    jobs: Vec<mpsc::Receiver<(String, bool)>>,
    steam_busy: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// a program being chosen to run in a game's prefix, for that game
    run_pick: Option<(String, mpsc::Receiver<Option<std::path::PathBuf>>)>,
    /// an added plugin asked about removing
    removing: Option<String>,
    mouse_icons: Option<(f32, f32, Vec<slint::Image>)>,
    /// the mouse's picture in layers, for a scale, and the scale being drawn
    mouse_layers: Option<(f32, MouseImages)>,
    mouse_drawing: Option<f32>,
    /// the button under the pointer, on the picture or on its words
    mouse_hover: Option<u8>,
    /// the chosen and hovered buttons the picture shows now
    mouse_shows: (Option<u8>, Option<u8>),
    /// the buttons held down on the mouse now, as its core tells them (bit
    /// n - 1 for its button n, see mouse::BUTTONS), shown held on its pictures
    mouse_down: u8,
    /// the lights' effects played on the picture, 30 frames a second, while they move
    mouse_frames: Option<slint::Timer>,
    /// the scene on (see scenes.rs), by id: chosen by hand, or by a game or app
    /// in front; from when it plays, its frames' timer, when the mouse was
    /// last sent its part (with the colors it was sent), the colors it gives
    /// now (each key by its place, the mouse's logo and wheel), and whether
    /// it is sending each device its part now (the keyboard, the mouse)
    scene: Option<String>,
    scene_t0: Instant,
    scene_timer: Option<slint::Timer>,
    scene_mouse: (Option<Instant>, [[u8; 3]; 2]),
    scene_colors: (HashMap<u32, [u8; 3]>, [Option<[u8; 3]>; 2]),
    scene_claim: (bool, bool),
    /// the scene a game or app in front switched from, to go back to as it leaves
    scene_back: Option<Option<String>>,
    /// the profile Ectomatter last put each plugin's device on (OpenRGB's), for the
    /// scene or for the app in front, by plugin/device; when they were last looked
    /// at; and the thread their calls go to, in the order they were made
    plugin_applied: HashMap<String, String>,
    /// a plugin's device switched to a profile by hand (by plugin/device), with what
    /// had Ectomatter switching it then (the scene playing, the app in front's
    /// choice): the hand's choice stays until that changes
    plugin_hand: HashMap<String, (Option<String>, Option<String>)>,
    scene_plugins_at: Instant,
    plugin_calls: Option<mpsc::Sender<(crate::core_host::Core, &'static str, serde_json::Value, String)>>,
    plugin_done: Option<mpsc::Receiver<()>>,
    /// the scene handed back for Ectomatter quitting: nothing more is sent
    scene_quit_done: bool,
    /// whether the scene on moves (a static one's frames change nothing on the page)
    scene_moves: bool,
    /// when a press last shut a menu or the color picker: its click closes nothing more
    press_closed: Option<Instant>,
    /// Home's Scenes menu, open: what each of its rows does
    scene_menu: Option<Vec<SceneRow>>,
    /// the scene form, open, and its dropdown, open
    scene_form: Option<SceneForm>,
    scene_form_menu: Option<FormMenu>,
    /// the devices out of the scene as its last frame went, the keyboard and
    /// the mouse: each while its own Lighting page shows
    desk_aside: (bool, bool),
    /// Add pad's menu open, or the chosen pad's Size menu
    pad_menu: Option<PadMenu>,
    /// the mouse in HDR on its page: its lamps and unlit picture, for the
    /// layers' scale; the picture lit as it shows now, and the part of the
    /// whole picture the page's hole shows of it (in pixels)
    mouse_hdr: Option<(f32, crate::mouse_art::Hdr)>,
    mouse_lit: Option<Rc<crate::picture::Lit>>,
    mouse_part: (usize, usize, usize, usize),
    /// the light the mouse is lit by while the lab tunes it (none: as its
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
    /// the window KWin last said was in front, looked at again when a game starts or quits
    last_front: Option<crate::front_app::Front>,
    /// the Screen tab's Game launch: the row whose layout the screen above shows (selected
    /// again, let go), the row a carried layout is over, the row whose until menu is open
    gl_pick: Option<usize>,
    gl_over: Option<usize>,
    until_menu: Option<usize>,
    /// the game starting, playing or quitting, or the last one, as its readings go to the screen
    game_now: Option<crate::lcd_page::Game>,
    /// Ectomatter's macros played, for the controls given one (made when first
    /// needed), and where its steps' jobs come back to be done (main.rs)
    typist: Option<crate::typist::Player>,
    pub macro_jobs: Option<smithay_client_toolkit::reexports::calloop::channel::Sender<String>>,
    /// a slider set to Nothing because the other took its job: the job it had
    idled: [Option<String>; 2],
    last_sliders: Option<serde_json::Value>,
    held: Option<u8>,
    sent: [Option<f64>; 2],
    /// quitting: when the keyboard was asked to take its sliders back, which
    /// Ectomatter waits a moment for before it goes
    released: Option<Instant>,
    /// Ectomatter's own settings; and a search in a plugin's own Settings
    /// section: whose box and which setting, with what is typed; when it was
    /// typed; and what the plugin found (and its words for finding nothing)
    pub settings: crate::settings::Settings,
    typing: Option<(String, String, String)>,
    typed_at: Option<Instant>,
    found: (Vec<serde_json::Value>, String),
    found_tx: mpsc::Sender<(String, Vec<serde_json::Value>, String)>,
    found_rx: mpsc::Receiver<(String, Vec<serde_json::Value>, String)>,
    search_menu: bool,
    /// the plugin sections' pieces on the Settings page, by place
    plugin_map: crate::ecto_settings::PluginMap,
    /// the white of the screen the window is on and the brightest screen's
    /// peak: the Peak nits slider's two ends, wherever the window is
    pub levels: (f64, f64),
    /// the peak of the screen the window is on: the keys' light goes no higher there
    pub screen_peak: f64,
    /// Ectomatter is to start again, on this place, for a change to what it may use
    pub restart: Option<String>,
    /// the Macro Editor's window, once it has been opened
    pub macros: Option<crate::macro_page::MacroPage>,
    /// the lab's window, once it has been opened
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
    /// each core's device as the system last showed it (plugged in, opened,
    /// who else has it open), and when: looked at every two seconds at most
    device_status: HashMap<String, (Instant, serde_json::Value)>,
    /// the screen: Ectomatter's player, what it last said, whether it is
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
    /// the Sensor Feed box in Settings: the tab open, the folded runs opened (by device and
    /// run), the sensor being renamed and what is typed, what its pieces stood for as last
    /// drawn; the computer's sensors and their readings, read once a second while it shows;
    /// and the sensor asked about before it is unchecked
    sensor_tab: ectomatter_sensors::list::Part,
    sensor_open: std::collections::BTreeSet<(String, ectomatter_sensors::list::Run)>,
    renaming: Option<(String, String)>,
    sensor_map: crate::ecto_settings::SensorMap,
    sensors_seen: Option<(Instant, ectomatter_sensors::list::Catalog, std::collections::HashMap<String, ectomatter_sensors::list::Val>)>,
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
        on!(on_action_menu, |l, x, y, h, w| Ask::ActionMenu(l, x, y, h, w));
        on!(on_listen, |l| Ask::Listen(l));
        on!(on_reset_key, Ask::Reset);
        on!(on_menu_chose, |i| Ask::MenuChose(i));
        on!(on_menu_field, |i, t| Ask::MenuField(i, t.to_string()));
        on!(on_menu_typed, |t| Ask::MenuTyped(t.to_string()));
        on!(on_menu_enter, Ask::MenuEnter);
        on!(on_menu_dismiss, Ask::MenuDismiss);
        on!(on_menu_back_to, Ask::MenuBack);
        on!(on_go, |p| Ask::Go(p.to_string()));
        on!(on_mouse_tab_chosen, |i| Ask::MouseTab(i));
        on!(on_boxes_resized, Ask::BoxesResized);
        // (a box's width kept as the window draws it, and its pieces laid out anew to it)
        let (light_boxes, mouse_boxes) = (crate::boxes::Row::default(), crate::boxes::Row::default());
        for (row, set) in [(light_boxes.widths(), 0), (mouse_boxes.widths(), 1)] {
            let a = asks.clone();
            let told = move |i: i32, w: f32| {
                if crate::boxes::noted(&row, i.max(0) as usize, w) {
                    a.borrow_mut().push(Ask::BoxesResized);
                }
            };
            if set == 0 {
                ui.on_light_box_resized(told);
            } else {
                ui.on_mouse_box_resized(told);
            }
        }
        on!(on_mouse_row, |i| Ask::MouseRow(i));
        on!(on_mouse_lib_toggle, |g| Ask::MouseLibToggle(g.to_string()));
        on!(on_mouse_lib_find, |t| Ask::MouseLibFind(t.to_string()));
        on!(on_mouse_kind_picked, |i, x, y, h| Ask::MouseKind(i, x, y, h));
        on!(on_mouse_job_clicked, |x, y, h, w| Ask::MouseJob(x, y, h, w));
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
        on!(on_desk_add_pad, |x, y, h| Ask::DeskAddPad(x, y, h));
        on!(on_desk_pad_size_menu, |x, y, h| Ask::DeskPadSize(x, y, h));
        on!(on_desk_draw_cancel, Ask::DeskDrawCancel);
        on!(on_scene_menu, |x, y, h| Ask::SceneMenu(x, y, h));
        on!(on_menu_edit, |i| Ask::MenuEdit(i));
        on!(on_scene_tile, |i| Ask::SceneTile(i));
        on!(on_scene_form_close, Ask::SceneFormClose);
        on!(on_scene_form_escape, Ask::SceneFormEscape);
        on!(on_scene_form_done, Ask::SceneFormDone);
        on!(on_scene_form_delete, Ask::SceneFormDelete);
        on!(on_mouse_effect, |i| Ask::MouseEffect(i));
        on!(on_rail, |p| Ask::Rail(p.to_string()));
        on!(on_gl_tab_chosen, |i| Ask::GlTab(i));
        on!(on_gl_find, |t| Ask::GlFind(t.to_string()));
        on!(on_gl_store_menu, |x, y, h, w| Ask::GlMenu(crate::games_page::Menu::Shop, x, y, h, w));
        on!(on_gl_sort_menu, |x, y, h, w| Ask::GlMenu(crate::games_page::Menu::Order, x, y, h, w));
        on!(on_gl_open, |i| Ask::GlOpen(i));
        on!(on_gl_star, |i| Ask::GlStar(i));
        on!(on_gl_every_page, Ask::GlEvery);
        on!(on_gl_add, Ask::GlAdd);
        on!(on_gl_back, Ask::GlBack);
        on!(on_gl_star_page, Ask::GlStarPage);
        on!(on_gl_play, Ask::GlPlay);
        on!(on_gl_choose_program, Ask::GlProgram);
        on!(on_gl_add_done, Ask::GlAddDone);
        on!(on_gl_modal_close, Ask::GlClose);
        on!(on_gl_seen_pick, |i| Ask::GlSeen(i));
        on!(on_gl_remove, Ask::GlRemove);
        on!(on_gl_undo, Ask::GlUndo);
        on!(on_gl_toc_jump, |i| Ask::GlToc(i));
        on!(on_gl_script_wait_toggle, Ask::GlScriptWait);
        on!(on_gl_script_keep_toggle, Ask::GlScriptKeep);
        on!(on_gl_script_limit_menu, |x, y, h, w| Ask::GlScriptLimit(x, y, h, w));
        on!(on_gl_script_remove, Ask::GlScriptRemove);
        on!(on_gl_script_typed, Ask::GlScriptTyped);
        on!(on_gl_var_remove, Ask::GlVarRemove);
        on!(on_gl_var_typed, Ask::GlVarTyped);
        on!(on_gl_plugin_choose_file, Ask::GlPluginFile);
        on!(on_gl_plugin_typed, Ask::GlPluginTyped);
        on!(on_gl_row_on, |i| Ask::GlRowOn(i));
        on!(on_gl_row_pick, |i| Ask::GlRowPick(i));
        on!(on_gl_row_until, |i, x, y, h, w| Ask::GlRowUntil(i, x, y, h, w));
        on!(on_gl_carry_move, |row, shelf| Ask::GlCarryMove(row, shelf));
        on!(on_macro_editor, |l| Ask::MacroEditor(l));
        on!(on_screen_editor, Ask::ScreenEditor);
        on!(on_effect, |i| Ask::Effect(i));
        on!(on_cell_pick, |id, i, x, y, h, w| Ask::CellPick(id, i, x, y, h, w));
        on!(on_cell_left, |id, i, kept| Ask::CellLeft(id, i, kept));
        on!(on_cell_slide, |id, v| Ask::CellSlide(id, v));
        on!(on_pk_sv, |x, y| Ask::PkSv(x, y));
        on!(on_pk_hue_at, |x| Ask::PkHue(x));
        on!(on_pk_field, |i, t| Ask::PkField(i, t.to_string()));
        on!(on_pk_well, |c| Ask::PkWell(c));
        on!(on_pk_enter, Ask::PkEnter);
        on!(on_cell_typed, |id, index, t| Ask::CellTyped(id, index, t.to_string()));
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
        let mut hosted = crate::hosted::Hosted::new(std::sync::Arc::new(crate::computer::ThisComputer::new()));
        // (each plugin's settings under the id it has now first, its switch among them)
        crate::packages::rename_settings(&settings);
        hosted.start_all(&settings);
        // (offscreen, nothing is changed: the thread has no monitors to reach)
        let screens: Option<Box<dyn crate::putback::Screens>> = if crate::desk::offscreen() { None } else { Some(Box::new(crate::monitors::Hands)) };
        let worker = crate::putback::Worker::start(crate::putback::Journal::load(crate::putback::kept_at()), screens);
        let mut f = Front {
            mouse: crate::mouse::Page::new(),
            games: crate::games_page::Games::new(&ui),
            ties_taken: [false, false],
            hosted,
            worker,
            settled: false,
            following: HashMap::new(),
            packages: Vec::new(),
            package_form: None,
            updates: crate::updates::Checker::start(),
            windows: HashMap::new(),
            app_sessions: Default::default(),
            facts_reader: None,
            known: None,
            known_asked: None,
            recipes_for: None,
            tool_installed: false,
            starting: HashMap::new(),
            holds: Vec::new(),
            early: Vec::new(),
            jobs: Vec::new(),
            steam_busy: None,
            run_pick: None,
            removing: None,
            mouse_icons: None,
            mouse_layers: None,
            mouse_drawing: None,
            mouse_hover: None,
            mouse_shows: (None, None),
            mouse_down: 0,
            mouse_frames: None,
            scene: None,
            scene_t0: Instant::now(),
            scene_timer: None,
            scene_mouse: (None, [[0; 3]; 2]),
            scene_colors: Default::default(),
            scene_claim: (false, false),
            scene_back: None,
            plugin_applied: HashMap::new(),
            plugin_hand: HashMap::new(),
            scene_plugins_at: Instant::now(),
            plugin_calls: None,
            plugin_done: None,
            scene_quit_done: false,
            scene_moves: false,
            press_closed: None,
            scene_menu: None,
            scene_form: None,
            scene_form_menu: None,
            desk_aside: (false, false),
            pad_menu: None,
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
            mouse_lib_shut: None,
            mouse_lib_query: String::new(),
            mouse_lib_revealed: None,
            mouse_lib_rows: HashMap::new(),
            screen_sent: None,
            slider_sel: 0,
            listening: None,
            shut: None,
            t0: Instant::now(),
            light_at: None,
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
            scene_cells: Rc::new(VecModel::default()),
            light_boxes,
            mouse_boxes,
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
            last_front: None,
            gl_pick: None,
            gl_over: None,
            until_menu: None,
            game_now: None,
            typist: None,
            macro_jobs: None,
            idled: crate::uistate::get("slider_idled").and_then(|v| serde_json::from_value(v).ok()).unwrap_or([None, None]),
            last_sliders: None,
            held: None,
            sent: [None, None],
            released: None,
            settings,
            typing: None,
            typed_at: None,
            found: (Vec::new(), String::new()),
            found_tx: ftx,
            found_rx: frx,
            search_menu: false,
            plugin_map: Default::default(),
            levels: (254.0, 1000.0),
            screen_peak: 1000.0,
            restart: None,
            macros: None,
            lab: None,
            last_reading: None,
            reading_gap: 120.0,
            device_status: HashMap::new(),
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
            sensor_tab: ectomatter_sensors::list::Part::Processor,
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
        // (a key's job, and a macro's step, kept by an old id from when a job named the
        // keyboard or the mouse in place of a device: kept by the device's key now)
        let mut renamed = false;
        for id in f.keyed.values_mut() {
            if let Some(new) = id.strip_prefix(crate::jobs::ON_KEY).and_then(crate::jobs::Job::renamed) {
                *id = format!("{}{new}", crate::jobs::ON_KEY);
                renamed = true;
            }
        }
        if renamed {
            let kept: serde_json::Map<String, serde_json::Value> = f.keyed.iter().map(|(c, id)| (c.to_string(), serde_json::json!(id))).collect();
            let _ = f.settings.set("devices.z10.screen.keyed", serde_json::Value::Object(kept));
        }
        crate::jobs::rename_in_macros();
        f.screens_noted = f.screens.clone();
        f.screens_saved = f.screens.clone();
        // (the weather Ectomatter kept itself before the Weather Feed plugin, the plugin's now)
        if let Some(old) = f.settings.take("weather").filter(|_| f.settings.get("plugins.weather-feed").is_none()) {
            let mut v = serde_json::json!({});
            for (key, from) in [("place", &old["place"]), ("temperature", &old["units"]["temperature"]), ("wind", &old["units"]["wind"])] {
                if !from.is_null() {
                    v[key] = from.clone();
                }
            }
            let _ = f.settings.set("plugins.weather-feed", v);
        }
        f.hand_values(None);
        // the scene, as it was left on (the setting Home kept before there were scenes, taken in as it)
        if let Some(old) = f.settings.take("desk.light") {
            if f.settings.get("scene").is_none() {
                let _ = f.settings.set("scene", old.as_str().and_then(crate::scenes::from_old_setting).map_or(serde_json::Value::Null, |s| serde_json::json!(s)));
            }
        }
        let on = f.settings.get("scene").and_then(|v| v.as_str().map(String::from));
        if on.is_some() && controlled {
            f.set_scene(on, false);
        }
        f.show_scene_button();
        f.ui.set_overlays(ModelRc::from(f.overlay_model.clone()));
        f.ui.set_pk_presets(ModelRc::new(VecModel::from(PRESETS.iter().map(|c| color(*c)).collect::<Vec<_>>())));
        f.ui.set_cells(ModelRc::from(f.cells.clone()));
        f.ui.set_scene_cells(ModelRc::from(f.scene_cells.clone()));
        f.ui.set_light_boxes(ModelRc::from(f.light_boxes.model.clone()));
        f.ui.set_mouse_boxes(ModelRc::from(f.mouse_boxes.model.clone()));
        f.ui.set_pl_blocks(ModelRc::from(f.blocks.clone()));
        f.poll_lcd();
        if controlled {
            f.ask_kb(device::Ask::State);
        } else {
            f.ui.set_conn_state(3);
            f.ui.set_conn_text("Not controlled by Ectomatter".into());
            f.ui.set_boot(3);
            f.ui.set_boot_title("Control is off".into());
            f.ui.set_boot_text("Ectomatter leaves this keyboard to other programs.".into());
        }
        f.give_packages();
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
        // its screen's light and screens too: what Ectomatter last sent is no longer there
        if matches!(h, device::Heard::Saved(..) | device::Heard::KeysSaved(..) | device::Heard::Active(..)) {
            self.screen_sent = None;
        }
        match h {
            device::Heard::Described(d) => {
                self.page.desc = d;
                self.icons = None;
            }
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
                if let Ok(dir) = std::env::var("ECTOMATTER_DUMP") {
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
                    self.ui.set_boot_text(format!("{e}. Check that it is plugged in; Ectomatter tries again every few seconds.").into());
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
            device::Heard::Input(p, down) => {
                self.keyboard_input(&p, down);
                return;
            }
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

    /// A search in a plugin's box: once typing has paused, what the plugin
    /// finds for the words, asked away from the window's thread, since it may
    /// wait on the network.
    fn search_plugin(&mut self) {
        if let Some(t) = self.typed_at {
            if t.elapsed() >= Duration::from_millis(300) {
                self.typed_at = None;
                let Some((plugin, setting, text)) = self.typing.clone() else { return };
                let q = text.trim().to_string();
                if q.is_empty() {
                    self.close_search_menu();
                    return;
                }
                let Some(core) = self.hosted.core_of(&plugin) else { return };
                let tx = self.found_tx.clone();
                std::thread::spawn(move || {
                    let answer = core.call("settings/search", &serde_json::json!({ "id": setting, "text": q })).unwrap_or_default();
                    let found = answer["found"].as_array().cloned().unwrap_or_default();
                    let _ = tx.send((q, found, answer["none"].as_str().unwrap_or("Nothing found").to_string()));
                });
            }
        }
        while let Ok((q, found, none)) = self.found_rx.try_recv() {
            if self.typing.as_ref().is_none_or(|(_, _, t)| q != t.trim()) {
                continue; // typed on since
            }
            self.found = (found, none);
            self.open_search_menu();
        }
    }

    fn open_search_menu(&mut self) {
        let rows: Vec<MenuRow> = if self.found.0.is_empty() {
            vec![MenuRow { kind: 1, label: self.found.1.clone().into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() }]
        } else {
            self.found
                .0
                .iter()
                .map(|f| MenuRow { kind: 0, label: f["label"].as_str().unwrap_or("").into(), tag: "".into(), tick: false, sub: f["sub"].as_str().unwrap_or("").into(), ..Default::default() })
                .collect()
        };
        // under the box typed in, at least as wide as it (at most PLUGIN_MENU_MOST of its
        // rows; the rest scroll), with nothing to tick
        let at = self.typing.as_ref().and_then(|(p, s, _)| self.plugin_map.fields.iter().position(|(fp, fs)| fp == p && fs == s)).map(|i| i as i32);
        let field = (0..self.cells.row_count()).filter_map(|i| self.cells.row_data(i)).find(|c| c.kind == lighting::kind::FIELD && lighting::ctl(c.id) == Ctl::PluginField && Some(c.index) == at);
        let Some(f) = field else { return };
        let scroll = self.ui.get_settings_scroll();
        // (the page under the title bar and its own bar)
        let (x, y) = (self.ui.get_rail_width() + f.x, crate::tokens::size::TITLE_BAR + crate::tokens::size::BAR + f.y + scroll);
        self.ui.set_menu_marks(false);
        self.hang_menu_most(rows, f.w, (x, y, f.h), PLUGIN_MENU_MOST);
        self.search_menu = true;
    }

    fn close_search_menu(&mut self) {
        if self.search_menu {
            self.search_menu = false;
            self.ui.set_menu_open(false);
            self.ui.set_menu_marks(true);
            self.ui.set_menu_search(true);
        }
    }

    /// Do what the page asked; what only the window can do is handed back.
    pub fn act(&mut self) -> Vec<WindowAsk> {
        self.search_plugin();
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
                    if let Some(id) = self.page.effects().get(i.max(0) as usize).and_then(|e| e["id"].as_str().map(String::from)) {
                        self.page.set_effect(&id);
                        // (the desk matches what was just chosen: the scene lighting it goes off)
                        if self.scene_lighting("z10").is_some() {
                            self.set_scene(None, true);
                        }
                    }
                    self.push_preview();
                }
                Ask::MouseTab(i) => {
                    self.close_menu();
                    self.close_picker();
                    self.mouse.tab = i.clamp(0, 3) as usize;
                }
                // (the refresh lays them out to their new widths)
                Ask::BoxesResized => {}
                Ask::MouseRow(i) => {
                    self.close_menu();
                    self.close_picker();
                    if self.mouse.tab == 1 {
                        // (the rows in the core's order, as the lights are on the mouse)
                        if let Some((l, ..)) = self.mouse.light_order().get(i.max(0) as usize) {
                            self.mouse.light = *l;
                        }
                    } else {
                        // selecting the chosen button again lets it go
                        let b = (i.max(0) as usize).min(7);
                        self.mouse.button = (self.mouse.button != Some(b)).then_some(b);
                    }
                }
                Ask::MouseLibToggle(g) => {
                    let shut = self.mouse_lib_shut.get_or_insert_with(Default::default);
                    if !shut.remove(&g) {
                        shut.insert(g);
                    }
                }
                Ask::MouseLibFind(t) => self.mouse_lib_query = t,
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
                        self.open_mouse_menu(k, (x, y, h), 0.0);
                    }
                }
                Ask::MouseJob(x, y, h, w) => {
                    if let Some(b) = self.mouse.button {
                        let k = crate::mouse::kind_of(&self.mouse.job(b));
                        self.open_mouse_menu(k, (x, y, h), w);
                    }
                }
                Ask::MouseReset => {
                    if let Some(b) = self.mouse.button {
                        self.mouse.reset_job(b);
                    }
                }
                Ask::MouseResetAll => {
                    // (every button, while the mouse can't say what they do)
                    let all = !self.mouse.buttons_known();
                    for b in 0..crate::mouse::BUTTONS.len() {
                        if all || self.mouse.rebound(b) {
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
                    window.push(WindowAsk::OpenMacros(crate::macros::Lib::Ectomatter, n));
                }
                Ask::MouseEffect(i) => {
                    if let Some(e) = self.mouse.effects().get(i.max(0) as usize) {
                        let old = self.mouse.light_of(self.mouse.light_name());
                        self.mouse.set_light(crate::described::with_effect(&old, e));
                        if self.scene_lighting("sensei310").is_some() {
                            self.set_scene(None, true);
                        }
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
                        // (a press on the empty desk, outside Arrange, sends a ripple from there)
                        if i < 0 && !self.desk.editing {
                            self.ripple_at((x, y), Duration::ZERO);
                        }
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
                Ask::SceneMenu(x, y, h) => self.open_scene_menu((x, y, h)),
                Ask::MenuEdit(i) => {
                    if let Some(SceneRow::Scene(id)) = self.scene_menu.as_ref().and_then(|a| a.get(i.max(0) as usize)).cloned() {
                        self.close_menu();
                        self.open_scene_form(Some(id));
                    }
                }
                Ask::SceneTile(i) => {
                    if let (Some(f), Some((e, ..))) = (&mut self.scene_form, crate::scenes::EFFECTS.get(i.max(0) as usize)) {
                        f.look = f.look.as_effect(*e);
                    }
                    self.show_scene_form();
                }
                Ask::SceneFormClose => {
                    // (a press that shut a menu or the picker shuts only that)
                    if !self.press_closed.take().is_some_and(|t| t.elapsed() < Duration::from_millis(700)) {
                        self.close_scene_form(false);
                    }
                }
                Ask::SceneFormEscape => self.scene_form_escape(),
                Ask::SceneFormDone => self.close_scene_form(true),
                Ask::SceneFormDelete => self.delete_scene(),
                Ask::SceneFrame => {
                    changed = false;
                    self.scene_frame();
                }
                Ask::DeskTick => {
                    changed = false;
                    let moving = self.desk.animating();
                    self.desk.tick(Instant::now());
                    // (a ripple changes only what the window lays on the ground,
                    // which it reads each frame; the desk's own parts move only
                    // while it eases or fades)
                    if moving {
                        self.show_desk();
                    }
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
                Ask::DeskAddPad(x, y, h) => {
                    changed = false;
                    self.open_pad_menu(PadMenu::Add, (x, y, h));
                }
                Ask::DeskPadSize(x, y, h) => {
                    changed = false;
                    if self.desk.pad_chosen.is_some() {
                        self.open_pad_menu(PadMenu::Size, (x, y, h));
                    }
                }
                Ask::DeskDrawCancel => {
                    changed = false;
                    self.desk.draw_tool = false;
                    self.show_desk();
                }
                // (a plugin's value on a Library page, its box left: kept, or put back)
                Ask::CellLeft(id, _, kept) => {
                    if lighting::ctl(id) == Ctl::LibText {
                        self.games.text_done(if kept { 1 } else { 2 });
                    }
                }
                Ask::CellPick(id, i, x, y, h, w) => match crate::described::piece_of(id) {
                    Some(piece) if self.on_keyboard() => self.kb_described_pick(piece, i.max(0) as usize, (x, y, h), w),
                    Some(piece) => self.described_pick(piece, i.max(0) as usize, (x, y, h)),
                    None => self.cell_pick(lighting::ctl(id), i.max(0) as usize, (x, y, h), w),
                },
                // (a setting a core described: the value its slider stands for)
                Ask::CellSlide(id, v) if crate::described::piece_of(id).is_some() => {
                    let piece = crate::described::piece_of(id).unwrap();
                    if self.on_keyboard() {
                        if let Some((sid, d, _)) = self.page.described(piece) {
                            let nv = crate::described::slid_kept(&sid, &d, v);
                            self.page.change_settings(|e| crate::described::keep(e, &sid, &d, &nv));
                            self.push_preview();
                        }
                    } else if let Some((sid, d, _)) = self.mouse.described(piece) {
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
                        Ctl::SceneSpeed => {
                            if let Some(f) = &mut self.scene_form {
                                f.look.speed = v.clamp(0.0, 1.0);
                            }
                            self.show_scene_form();
                        }
                        _ => {}
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
                        if let Some(row) = self.gl_over.take() {
                            self.set_game_launch(Self::launch_key(row as i32), "layout", serde_json::json!(c.page));
                            self.gl_pick = Some(row);
                            self.lcd_drawn = None;
                        } else if let Some(to) = c.shelf {
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
                    self.gl_over = None;
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
                    if let Some(id) = self.removing.take() {
                        self.remove_package(&id);
                    } else if let (Some(id), Some((_, cat, _))) = (self.unchecking.take(), self.sensors_seen.clone()) {
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
                    self.removing = None;
                    self.ui.set_confirm_open(false);
                }
                Ask::LcdTick => {
                    // the Library: Steam read, the games started and quit, the pictures drawn
                    let showing = self.ui.get_place() == "library";
                    let (events, mut news) = self.games.poll(showing);
                    if !self.settled && self.games.looked() {
                        self.settled = true;
                        self.worker.settle(self.games.running.clone());
                    }
                    if !events.is_empty() {
                        self.games_seen(events);
                    }
                    if self.hosted.poll() {
                        news = true;
                        if self.ui.get_place() == "settings" {
                            self.refresh();
                        }
                    }
                    // (a device a plugin met again, OpenRGB's after its program restarted, put on its profile afresh)
                    let met = self.hosted.take_added();
                    if !met.is_empty() {
                        for k in &met {
                            self.plugin_applied.remove(k);
                        }
                        self.plugins_sync();
                    }
                    news |= self.worker_told();
                    news |= self.launch_tick();
                    news |= self.games.typed_settled();
                    if self.package_form.is_some() {
                        self.package_form_tick();
                    }
                    while let Ok(id) = self.updates.updated.try_recv() {
                        // an update put in place: its plugin started again on it
                        self.give_packages();
                        if let Some(p) = self.packages.iter().find(|p| p.id() == id).cloned() {
                            if self.hosted.get(&id).is_some() {
                                self.hosted.stop(&id);
                                self.start_package(p);
                            }
                        }
                        if self.ui.get_place() == "settings" {
                            self.refresh();
                        }
                    }
                    if news && showing {
                        self.show_library();
                    }
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
                    // the Sensor Feed box's readings, once a second while Settings is open
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
                            let screen = self.page.sel.map(|pos| self.page.def(LAYERS[self.layer].0, pos)).filter(|d| !self.is_job_key(d)).and_then(|d| z10::screen_job(&d)).map_or(LAYOUT_PAGE, screen_page);
                            let page = [0, 1, 2, MACRO_PAGE, 3, screen, 6][i.clamp(0, 6) as usize];
                            self.open_menu_on(self.layer, (x, y, h), Some(page), 0.0);
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
                Ask::CellTyped(id, index, t) => match lighting::ctl(id) {
                    // (the name keeps what's typed; the box isn't drawn again under it)
                    Ctl::SceneName => {
                        if let Some(f) = &mut self.scene_form {
                            f.name = t;
                        }
                        changed = false;
                    }
                    Ctl::Name => crate::display::act(&mut self.page, Ctl::Name, 0, 0.0, Some(&t)),
                    // (the line keeps what's typed; the box isn't drawn again under it)
                    Ctl::SensorName => {
                        if let Some((_, typed)) = &mut self.renaming {
                            *typed = t;
                        }
                        changed = false;
                    }
                    // (a plugin's value on a Library page: kept as typed, set as its box is left)
                    Ctl::LibText => {
                        self.games.typing(index as usize, t);
                        changed = false;
                    }
                    Ctl::PluginField => {
                        // searched once typing pauses, as the web page did
                        if let Some((plugin, setting)) = self.plugin_map.fields.get(index as usize).cloned() {
                            self.typing = Some((plugin, setting, t));
                            self.typed_at = Some(Instant::now());
                        }
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
                Ask::ActionMenu(l, x, y, h, w) => self.open_menu(l as usize, (x, y, h), w),
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
                Ask::MenuField(i, t) => self.menu_field(i.max(0) as usize, &t),
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
                Ask::GlTab(i) => {
                    self.close_menu();
                    self.games.tab_chosen(i, &self.ui);
                }
                Ask::GlFind(t) => self.games.find(&t),
                Ask::GlMenu(m, x, y, h, w) => self.open_games_menu(m, (x, y, h), w),
                Ask::GlOpen(i) => {
                    self.close_menu();
                    self.games.open_at(i.max(0) as usize);
                }
                Ask::GlStar(i) => self.games.star_at(i.max(0) as usize),
                Ask::GlEvery => {
                    self.close_menu();
                    self.games.every();
                }
                Ask::GlAdd => {
                    self.close_menu();
                    self.games.add(&self.ui);
                }
                Ask::GlBack => {
                    self.close_menu();
                    self.games.back();
                }
                Ask::GlStarPage => self.games.star_page(),
                Ask::GlPlay => self.games.play(&self.front_apps),
                Ask::GlProgram => self.games.choose_program(),
                Ask::GlAddDone => {
                    if self.package_form.is_some() {
                        self.package_add_done();
                    } else {
                        self.games.add_done(&self.ui);
                    }
                }
                Ask::GlClose => {
                    if self.package_form.take().is_some() {
                        self.ui.set_gl_modal(0);
                    } else {
                        self.games.close_form();
                    }
                }
                Ask::GlPluginFile => {
                    if let Some(f) = &mut self.package_form {
                        if f.picking.is_none() {
                            // (offscreen, ECTOMATTER_PLUGIN_FILE's file, as if chosen there)
                            f.picking = Some(match std::env::var_os("ECTOMATTER_PLUGIN_FILE").filter(|_| crate::desk::offscreen()) {
                                Some(file) => {
                                    let (tx, rx) = mpsc::channel();
                                    let _ = tx.send(Some(std::path::PathBuf::from(file)));
                                    rx
                                }
                                None => crate::picker::open_file("Choose a plugin’s file", "Plugins", &["*.ectocore", "*.zip"]),
                            });
                        }
                    }
                }
                Ask::GlPluginTyped => {
                    if let Some(f) = &mut self.package_form {
                        f.typed_at = Some(Instant::now());
                        f.file = None;
                    }
                    self.show_package_form();
                }
                Ask::GlSeen(i) => self.games.seen_pick(i),
                Ask::GlRemove => {
                    self.close_menu();
                    self.games.remove(&self.front_apps);
                }
                Ask::GlUndo => self.games.undo(),
                Ask::GlToc(i) => self.games.toc_jump(i.max(0) as usize, &self.ui),
                Ask::GlScriptWait => self.games.script_wait(),
                Ask::GlScriptKeep => self.games.script_keep(),
                Ask::GlScriptLimit(x, y, h, w) => self.open_games_menu(crate::games_page::Menu::Limit, (x, y, h), w),
                Ask::GlScriptRemove => {
                    self.close_menu();
                    self.games.remove_script();
                }
                // (Add turns on once a command is typed)
                Ask::GlScriptTyped => {}
                Ask::GlVarRemove => {
                    self.close_menu();
                    self.games.remove_var();
                }
                Ask::GlVarTyped => {}
                Ask::GlRowOn(i) => {
                    let k = Self::launch_key(i);
                    let on = !self.game_launch_value(k)["on"].as_bool().unwrap_or(true);
                    self.set_game_launch(k, "on", serde_json::json!(on));
                    if !on && self.gl_pick == Some(i.max(0) as usize) {
                        self.gl_pick = None;
                    }
                    self.lcd_drawn = None;
                }
                Ask::GlRowPick(i) => {
                    let i = i.max(0) as usize;
                    self.gl_pick = if self.gl_pick == Some(i) { None } else { Some(i) };
                    self.lcd_drawn = None;
                }
                Ask::GlRowUntil(i, x, y, h, w) => self.open_until_menu(i.max(0) as usize, (x, y, h), w),
                Ask::GlCarryMove(row, shelf) => {
                    changed = false;
                    let n = self.screens.library.len();
                    if let Some(c) = self.carry.as_mut().filter(|c| !c.from_strip) {
                        let at = usize::try_from(shelf).ok().filter(|_| n > 0).map(|s| s.min(n - 1));
                        if at != c.shelf {
                            c.shelf = at;
                            changed = true;
                        }
                    }
                    let over = usize::try_from(row).ok().filter(|_| self.carry.as_ref().is_some_and(|c| !c.from_strip));
                    if over != self.gl_over {
                        self.gl_over = over;
                        self.lcd_drawn = None;
                        changed = true;
                    }
                }
                Ask::Go(place) => {
                    self.close_menu();
                    self.ui.set_place(place.into());
                }
                // the item already chosen opens the rail into its blade, or closes it
                // the lab is a window of its own, not a place in this one
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
        self.pale = controls.iter().filter_map(|c| Some((c["id"].as_str()?, c["pos"].as_u64()? as u32))).filter(|(id, _)| ectomatter_draw::hdr::pale_top(light, id)).map(|(_, pos)| pos).collect();
    }

    /// Do what the lab asked; what only its window can do is handed back.
    /// (The picker's answer for a typeface, not a part's picture.)
    pub fn act_lab(&mut self) -> Vec<WindowAsk> {
        let Some(l) = &mut self.lab else { return Vec::new() };
        let window = l.act();
        // the light setup chosen to light the keyboard, kept for next time
        if let Some(setup) = l.switched.take() {
            let _ = self.settings.set(crate::setups::IN_USE, setup.map_or(serde_json::Value::Null, serde_json::Value::from));
        }
        // a picture asked for: the desktop's own file picker, and the file
        // chosen there put on the key, kept in Ectomatter's data
        if let Some(part) = l.image_wanted.take() {
            self.picking = Some((part, crate::picker::open_file("Choose a picture for the key", "SVG pictures", &["*.svg", "*.SVG"])));
        }
        // a typeface of the person's own asked for: the same picker, the file
        // chosen kept in Ectomatter's typefaces and the legends set in it
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
        // (where Ectomatter's macros run, as the mouse's profiles say now)
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
    /// than stale. (While Ectomatter's own player drives the screen, the picture follows each
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

    /// Whether Ectomatter's own player is what the keyboard's screen shows now.
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

    /// The window closing or Ectomatter quitting: the keyboard goes back to what its
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
        // (while a scene sends the keyboard its colors, a preview left running waits: it
        // goes on again as the scene lets the keyboard go)
        if !self.previewing || self.page.dev.is_none() || self.kb.is_none() || self.scene_claim.0 {
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

    /// Whether the keyboard's page is the one showing, whose Lighting boxes
    /// are drawn from its description.
    fn on_keyboard(&self) -> bool {
        self.ui.get_place() == "z10"
    }

    /// A piece of the keyboard's Lighting boxes, drawn from its description,
    /// used: a color opens the color picker, a menu choice its menu, the
    /// zones their switch or a zone, and a choice or a checkbox takes its
    /// new value.
    fn kb_described_pick(&mut self, piece: crate::described::Piece, index: usize, anchor: (f32, f32, f32), under: f32) {
        let Some((id, d, e)) = self.page.described(piece) else { return };
        match d["type"].as_str().unwrap_or("") {
            "color" | "colors" => {
                self.open_picker(Ctl::Described, Some(piece), index, anchor);
                return;
            }
            "choice" if d["menu"] == true => {
                self.open_light_menu(piece, anchor, under);
                return;
            }
            "zones" => {
                let light = self.page.keys_light();
                let areas = self.areas.entry((self.page.editing, self.page.effect_id())).or_insert(crate::described::all_zones(&light));
                let z = crate::described::zones_picked(&light, crate::described::kept(&e, &id, &d).as_u64().unwrap_or(0), index, areas);
                self.page.change_settings(|e| crate::described::keep(e, &id, &d, &serde_json::json!(z)));
            }
            _ => {
                if let Some(v) = crate::described::picked(&d, &crate::described::kept(&e, &id, &d), index) {
                    self.page.change_settings(|e| crate::described::keep(e, &id, &d, &v));
                }
            }
        }
        self.push_preview();
    }

    /// On Some zones, a zone turned on or off from the keyboard's picture, as its chip does.
    fn kb_zone(&mut self, zone: usize) {
        let effect = self.page.effect_desc(&self.page.effect_id());
        if let Some(k) = crate::described::params(&effect).iter().position(|(_, d)| d["type"] == "zones") {
            self.kb_described_pick(crate::described::Piece::Effect(k), zone, (0.0, 0.0, 0.0), 0.0);
        }
    }

    /// A piece of the tile used: a swatch opens the color picker, the rest change the draft.
    fn cell_pick(&mut self, c: Ctl, index: usize, _anchor: (f32, f32, f32), under: f32) {
        match c {
            Ctl::SceneName if index == 2 => {
                self.scene_form_escape();
                return;
            }
            Ctl::SceneLights => {
                if let Some(f) = &mut self.scene_form {
                    f.desk = index == 0;
                }
                self.show_scene_form();
                self.scene_play();
                return;
            }
            Ctl::SceneWay => {
                if let Some(f) = &mut self.scene_form {
                    f.look.reverse = index == 1;
                }
                self.show_scene_form();
                return;
            }
            Ctl::SceneColor => {
                self.open_picker(Ctl::SceneColor, None, index, _anchor);
                return;
            }
            Ctl::ScenePart => {
                if let Some((key, ..)) = self.scene_form_devices().get(index).cloned() {
                    self.open_scene_form_menu(FormMenu::Part(key), _anchor, under);
                }
                return;
            }
            Ctl::ScenePlugin => {
                if let Some((dev, ..)) = self.scene_form_plugins().get(index).cloned() {
                    self.open_scene_form_menu(FormMenu::Plugin(dev), _anchor, under);
                }
                return;
            }
            Ctl::LibChoice => {
                self.open_games_menu(crate::games_page::Menu::Device(index), _anchor, under);
                self.refresh();
                return;
            }
            Ctl::LibSetting => {
                self.open_games_menu(crate::games_page::Menu::Setting(index), _anchor, under);
                self.refresh();
                return;
            }
            Ctl::LibScript => {
                self.close_menu();
                self.games.edit_script(index, &self.ui);
                self.refresh();
                return;
            }
            Ctl::LibScriptRun => {
                if let Some(s) = self.games.run_now(index) {
                    self.worker.try_now(s);
                }
                self.refresh();
                return;
            }
            Ctl::LibScriptAdd => {
                self.close_menu();
                self.games.add_script(&self.ui);
                self.refresh();
                return;
            }
            Ctl::LibSpot => {
                self.close_menu();
                if let Some((plugin, action)) = self.games.spot_picked(index, &self.ui) {
                    if self.hosted.launcher() == Some(plugin.as_str()) {
                        self.launch_action(&action);
                    }
                }
                self.refresh();
                return;
            }
            Ctl::LibCheck => {
                self.close_menu();
                self.games.check_picked(index);
                self.refresh();
                return;
            }
            Ctl::LibReset => {
                self.close_menu();
                self.games.reset_picked(index);
                self.refresh();
                return;
            }
            Ctl::PackageOn => {
                self.package_switch(index / 2, index % 2 == 0);
                self.refresh();
                return;
            }
            Ctl::PackageAdd => {
                self.close_menu();
                self.add_package_form(if index == 0 { crate::packages::Kind::Core } else { crate::packages::Kind::Plugin });
                self.refresh();
                return;
            }
            Ctl::PackageRemove => {
                // first the question, saying what's left after
                let Some(p) = self.packages.get(index).cloned() else { return };
                let after = if crate::packages::all_in(&crate::cores::dir(), std::path::Path::new("")).iter().any(|b| b.id() == p.id()) {
                    format!("Ectomatter goes back to the {} that comes with it.", p.name())
                } else {
                    format!("Its sections and devices go from Ectomatter, and what it changed for a game running is put back. The choices made in its sections are kept, for if you add {} again.", p.name())
                };
                self.ui.set_confirm_title(format!("Remove {} {}?", p.name(), p.version()).into());
                self.ui.set_confirm_body(after.into());
                self.ui.set_confirm_keep("Keep".into());
                self.ui.set_confirm_do("Remove".into());
                self.removing = Some(p.id().to_string());
                self.ui.set_confirm_open(true);
                return;
            }

            Ctl::MouseSameWheel => {
                self.mouse.same_wheel = index % 2 == 1;
                crate::uistate::set("mouse_same_wheel", serde_json::json!(self.mouse.same_wheel));
                if self.mouse.same_wheel {
                    self.mouse.copy_logo_to_wheel();
                }
                self.refresh();
                return;
            }
            Ctl::Paint | Ctl::LcdColor => {
                self.open_picker(c, None, index, _anchor);
                return;
            }
            Ctl::Reset => self.page.reset_effect(),
            Ctl::Preset => {
                if let Some(c) = lighting::PRESETS.get(index) {
                    self.paint = *c;
                }
            }
            // a checkbox: which box, and the state it is turned to
            Ctl::Demo => self.page.demo = index % 2 == 1,
            Ctl::LcdApp | Ctl::GameKey => crate::display::act(&mut self.page, c, index / 2, (index % 2) as f32, None),
            Ctl::LcdPreset => crate::display::act(&mut self.page, c, index, 0.0, None),
            Ctl::SliderBind => {
                self.open_bind_menu(index, _anchor, under);
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
            Ctl::PluginChoice => {
                if let Some((plugin, setting, v)) = self.plugin_map.choices.get(index).cloned() {
                    if self.settings.get(&format!("plugins.{plugin}.{setting}")).as_ref() != Some(&v) {
                        self.set_plugin_value(&plugin, &setting, v);
                    }
                }
                return;
            }
            Ctl::Control | Ctl::HdrMode => {
                let Some(v) = crate::ecto_settings::choice(c, index) else { return };
                let path = match c {
                    Ctl::Control => "devices.z10.control",
                    _ => "hdr.lighting",
                };
                if self.settings.get(path).as_ref() == Some(&v) || (c == Ctl::Control && v == serde_json::json!(self.controlled)) {
                    return;
                }
                if c == Ctl::Control && v == serde_json::json!(false) {
                    // give the keyboard back what Ectomatter borrowed before letting it go
                    self.ask_kb(device::Ask::Modes(false, false));
                    self.ask_kb(device::Ask::Dimmer(None));
                    self.stop_preview();
                }
                if let Err(e) = self.settings.set(path, v) {
                    self.status(format!("The setting was not saved: {e}"), 2);
                    return;
                }
                // a change to what Ectomatter may use starts it again, back on Settings
                if c == Ctl::Control {
                    self.restart = Some("settings".into());
                }
            }
            Ctl::Fill | Ctl::Scatter | Ctl::Clear => {
                // Fill paints an area (the whole keyboard, or one of the others); the rest every key
                let area = if c == Ctl::Fill { crate::described::zone_bit(&self.page.keys_light(), index).unwrap_or(0x80) as u8 } else { 0x80 };
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
            _ => {}
        }
        self.push_preview();
    }

    // ------------------------------------------------------------ the color picker

    fn open_picker(&mut self, c: Ctl, piece: Option<crate::described::Piece>, index: usize, anchor: (f32, f32, f32)) {
        // the second click on the swatch whose picker a press just closed
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let described = match piece {
            Some(p) if self.on_keyboard() => self.page.described(p).map(|(id, d, e)| {
                let v = crate::described::kept(&e, &id, &d);
                (id, d, v)
            }),
            Some(p) => self.mouse.described(p),
            None => None,
        };
        let rgb = match described {
            Some((_, d, v)) => crate::described::color_at(&d, &v, index).unwrap_or([0xff, 0, 0]),
            None if c == Ctl::DeskPad => self.desk.pad_chosen.and_then(|i| self.desk.pads.get(i)).map_or(crate::desk::PAD_COLOR, |p| p.color),
            None if c == Ctl::SceneColor => self.scene_form_color(index).unwrap_or([0xff, 0x60, 0]),
            None => lighting::color_of(&self.page, c, self.paint),
        };
        self.pick = Some(Pick { ctl: c, piece, index, start: rgb, hsv: rgb2hsv(rgb), rgb, anchor });
        let recent = recent_colors();
        self.ui.set_pk_recent(ModelRc::new(VecModel::from(recent.iter().map(|c| color(*c)).collect::<Vec<_>>())));
        // under the swatch when it fits there, or when there is a fair amount of room there anyway; over it when below is cramped
        let (ax, ay, ah) = anchor;
        let (h, win) = (self.ui.get_picker_height(), self.ui.get_window_height());
        let (top, _) = crate::menu_size::hang(win, (ay, ah), h);
        self.ui.set_picker_x(ax);
        self.ui.set_picker_top(top);
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
        } else if c == Ctl::SceneColor {
            self.set_scene_form_color(index, rgb);
        } else if c == Ctl::DeskPad {
            self.desk.set_pad_color(rgb);
            self.show_desk();
        } else if let Some(piece) = piece.filter(|_| self.on_keyboard()) {
            if let Some((id, d, _)) = self.page.described(piece) {
                self.page.change_settings(|e| crate::described::keep_color(e, &id, &d, index, rgb));
            }
        } else if let Some(piece) = piece {
            if let Some((_, d, v)) = self.mouse.described(piece) {
                self.mouse.set_described(piece, crate::described::with_color(&d, &v, index, rgb));
            }
            self.refresh();
        } else {
            lighting::set_color(&mut self.page, c, rgb);
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

    /// Ectomatter closing: whatever has changed is saved now, without waiting.
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
            // painting key by key: a click paints the key
            if self.page.effect_desc(&self.page.effect_id())["paint"] == true {
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
                    self.kb_zone(i);
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


    /// A key typed into Ectomatter's window, as a Linux key code, which "Press
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
    /// A key pressed or let go on the keyboard, as its core tells it: one on
    /// a host slot does its job; each shows on the keyboard's picture, at the
    /// key that sends what it sent.
    pub fn keyboard_input(&mut self, p: &serde_json::Value, down: bool) {
        if let Some(n) = p["host"].as_u64() {
            self.screen_key_event(z10::NEXT_LAYOUT + n as u32, down);
        }
        let Some(sent) = Action::read(&p["sends"]) else { return };
        let Some(pos) = self.page.pos_for(&sent).filter(|p| self.page.key(*p).is_some()) else { return };
        if down {
            if self.page.pressed.insert(pos) {
                let now = self.t0.elapsed().as_secs_f64() * 1000.0;
                self.page.flash(pos, now);
                self.ripple_from_key(pos, Duration::ZERO);
                self.refresh();
            }
        } else if self.page.pressed.remove(&pos) {
            self.refresh();
        }
    }

    /// A key pressed on the keyboard Home shows: a ripple runs out across the
    /// desk from the key, `age` into its run.
    pub fn ripple_from_key(&mut self, pos: u32, age: Duration) {
        let Some(k) = self.page.key(pos) else { return };
        let Some(at) = self.on_desk_keyboard(k.cx, k.cy) else { return };
        self.ripple_at(at, age);
    }

    /// A mouse button pressed (by the core's name for it, see mouse::BUTTONS):
    /// a ripple from the button, where the mouse sits on Home's desk. Not the
    /// pointer's click (the button whose job is the mouse's own button 1): its
    /// ripple comes from where the pointer clicks the desk, and from nowhere
    /// when the click is elsewhere.
    pub fn ripple_from_button(&mut self, control: &str, age: Duration) {
        let Some(b) = crate::mouse::BUTTONS.iter().position(|(id, ..)| *id == control) else { return };
        if self.mouse.job(b)["mouse"].as_u64() == Some(1) {
            return;
        }
        let Some((dw, layers)) = self.desk_layers() else { return };
        let Some(&(x, y)) = crate::mouse_art::button_middles(layers, dw).get(b) else { return };
        let Some(at) = self.on_desk_mouse(x, y) else { return };
        self.ripple_at(at, age);
    }

    /// A ripple from a point of Home's desk (its logical pixels), `age` into
    /// its run, in the light nearest it as the devices show it now (see
    /// desk::ripple_light): none while Home doesn't show, or while the window
    /// is closed into the tray, where the desk isn't drawn.
    fn ripple_at(&mut self, at: (f32, f32), age: Duration) {
        if self.hidden || self.ui.get_place() != "home" {
            return;
        }
        let color = crate::desk::ripple_light(at, &self.lights_on_desk());
        self.desk.ripple(at, color, age);
        self.desk_frames_run();
    }

    /// Each light on Home's desk as it shows now: where it sits (the desk's
    /// logical pixels) and its color, none where it's dark. The keys as the
    /// keyboard was last lit (in the desk's light, while that plays), a key's
    /// Reactive flash from the moment its press starts it; the mouse's logo
    /// and wheel.
    fn lights_on_desk(&self) -> Vec<((f32, f32), Option<[u8; 3]>)> {
        let mut out = Vec::new();
        let drawn: HashMap<u32, &Light> = self.last_lights.iter().flatten().map(|l| (l.pos, l)).collect();
        let scene = !self.scene_colors.0.is_empty();
        for k in self.page.layout.iter().filter(|k| k.led) {
            let Some(at) = self.on_desk_keyboard(k.cx, k.cy) else { break };
            let color = if scene {
                self.scene_colors.0.get(&k.pos).copied()
            } else {
                self.page.flash_look(k.pos).or_else(|| drawn.get(&k.pos).filter(|l| l.strength > 0.0).map(|l| l.rgb))
            };
            out.push((at, color));
        }
        if let Some((dw, layers)) = self.desk_layers() {
            let now = Instant::now();
            for (i, b) in crate::mouse_art::light_boxes(layers, dw).iter().enumerate() {
                if let Some(at) = self.on_desk_mouse((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0) {
                    out.push((at, self.mouse_color(i, now)));
                }
            }
        }
        out
    }

    /// A point of the keyboard's drawing (the board's own units, as the
    /// page's view_box has them) where the keyboard sits on Home's desk, in
    /// the desk's logical pixels; none off Home.
    fn on_desk_keyboard(&self, x: f64, y: f64) -> Option<(f32, f32)> {
        let [kx, ky, kw, kh] = self.desk_keyboard()?;
        let (vx, vy, vw, vh) = self.page.view_box;
        Some((kx + ((x - vx) / vw) as f32 * kw - self.ui.get_desk_x(), ky + ((y - vy) / vh) as f32 * kh - self.ui.get_desk_y()))
    }

    /// The same for a point of the mouse's drawing (see mouse_art::VIEW).
    fn on_desk_mouse(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let i = self.desk.items.iter().position(|it| it.id == "mouse")?;
        let [mx, my, mw, mh] = self.desk.screen(i);
        let v = crate::mouse_art::VIEW;
        Some((mx + (x - v.0) / v.2 * mw, my + (y - v.1) / v.3 * mh))
    }

    /// The computer's lock lights now (bit 0 Num Lock, 1 Caps Lock, 2 Scroll
    /// Lock), as the keyboard shows them.
    pub fn locks(&mut self, bits: u8) {
        if self.page.locks != bits {
            self.page.locks = bits;
            // the keyboard is lit again with them
            self.last_lights = None;
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

    fn open_menu(&mut self, layer: usize, anchor: (f32, f32, f32), under: f32) {
        // the job's own kind of list, or the whole menu for a key that does nothing (a key
        // with a job Ectomatter does sends one of the codes a playlist could: its job's page)
        let page = self.page.sel.map(|pos| self.page.def(LAYERS[layer].0, pos)).and_then(|d| match if self.is_job_key(&d) { self.key_tag(&d) } else { z10::kind_of(&d) } {
            "Key" => Some(0),
            "Media" => Some(1),
            "Mouse" => Some(2),
            "Macro" => Some(MACRO_PAGE),
            "Profile" => Some(3),
            "Screen" => z10::screen_job(&d).map(screen_page),
            "Keyboard" => Some(6),
            _ => None,
        });
        self.open_menu_on(layer, anchor, page, under);
    }

    fn open_menu_on(&mut self, layer: usize, anchor: (f32, f32, f32), page: Option<usize>, under: f32) {
        let Some(pos) = self.page.sel else { return };
        // the second click on the button whose menu a press just closed
        if self.just_shut(anchor) {
            return;
        }
        if self.menu.as_ref().is_some_and(|m| m.layer == layer && m.page == page) {
            self.close_menu();
            return;
        }
        self.listening = None;
        self.menu = Some(Menu { layer, pos, page, query: String::new(), anchor, choices: Vec::new() });
        self.ui.set_menu_query("".into());
        // (with its search box, one width as it's searched: Size.menu-search, or its field's;
        // a column for the tick on the key's own job)
        self.ui.set_menu_search(true);
        self.ui.set_menu_marks(true);
        self.ui.set_menu_w(crate::tokens::size::MENU_SEARCH.max(under));
        self.build_menu();
        // under the button when it fits there, or when there is a fair amount of
        // room there anyway (the menu then scrolls); over it only when below is
        // cramped. Where it hangs is fixed now, so it stays put as it is searched.
        let (_, ay, ah) = anchor;
        let (h, win) = (self.ui.get_menu_natural(), self.ui.get_window_height());
        let (top, most) = crate::menu_size::hang(win, (ay, ah), h);
        if std::env::var_os("ECTOMATTER_DEBUG").is_some() {
            eprintln!("menu: natural {h} window {win} anchor {ay}+{ah} top {top} most {most}");
        }
        self.ui.set_menu_top(top);
        self.ui.set_menu_max(most);
        self.ui.set_menu_open(true);
        // (turned to what the key does now, where that's out of view)
        let rows: Vec<MenuRow> = self.ui.get_menu_rows().iter().collect();
        let head = crate::menu_size::height(&[], true, self.ui.get_menu_back(), 0.0);
        self.ui.invoke_scroll_menu(crate::menu_size::reveal(&rows, h.min(most) - head));
        self.ui.invoke_focus_menu();
    }

    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    fn close_menu(&mut self) {
        // (one of the scene form's dropdowns: shut, and drawn shut)
        if self.scene_form_menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.show_scene_form();
        }
        if self.bind_menu.take().is_some() || self.light_menu.take().is_some() || self.way_menu.take().is_some() || self.time_menu.take().is_some() || std::mem::take(&mut self.pl_menu) || std::mem::take(&mut self.add_menu) || std::mem::take(&mut self.ob_menu) || self.lib_menu.take().is_some() || self.new_menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
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
        if self.mouse.menu.take().is_some() || std::mem::take(&mut self.profile_menu) || std::mem::take(&mut self.kb_profile_menu) || self.apps_menu.take().is_some() || self.scene_menu.take().is_some() || self.pad_menu.take().is_some() || self.games.menu.take().is_some() || self.until_menu.take().is_some() {
            self.ui.set_menu_open(false);
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
        self.press_closed = None;
        self.let_chosen_go(x, y);
        if self.search_menu {
            let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
            if !(x >= mx && x < mx + mw && y >= my && y < my + mh) {
                self.close_search_menu();
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
        // (every menu the window hangs, whatever opened it, by one rule: a press off it shuts
        // it; the key's menu, which has a find box of its own, is shut below)
        if self.ui.get_menu_open() && self.menu.is_none() {
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
                if x >= ax && x < ax + aw && y >= ay && y < ay + ah {
                    self.shut = Some(((ax, ay), Instant::now()));
                }
                self.press_closed = Some(Instant::now());
                self.close_menu();
                self.refresh();
            }
            return;
        }
        // away from the open picker closes it; on the swatch it hangs from too
        if self.pick.is_some() {
            let (px, py, ph) = (self.ui.get_picker_x().max(crate::tokens::space::INLINE).min(self.ui.get_window_width() - crate::tokens::size::PICKER - crate::tokens::space::INLINE), self.ui.get_picker_top(), self.ui.get_picker_height());
            if !(x >= px && x < px + crate::tokens::size::PICKER && y >= py && y < py + ph) {
                let (ax, ay, ah) = self.pick.as_ref().unwrap().anchor;
                // (a swatch is 34 wide; a pad's Color button, its swatch and its word)
                let aw = if self.pick.as_ref().is_some_and(|p| p.ctl == Ctl::DeskPad) { crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CONTROL, "Color") + 42.0 } else { 34.0 };
                if x >= ax && x < ax + aw && y >= ay && y < ay + ah {
                    self.shut = Some(((ax, ay), Instant::now()));
                }
                self.close_picker();
                self.press_shut = true;
                self.press_closed = Some(Instant::now());
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
        if y < 36.0 || self.ui.get_menu_open() || self.menu.is_some() || self.mouse.menu.is_some() || self.profile_menu || self.kb_profile_menu || self.apps_menu.is_some() || self.scene_menu.is_some() || self.scene_form_menu.is_some() || self.pad_menu.is_some() || self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.pick.is_some() || self.search_menu {
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
                // (the row of boxes right of the Profiles box: the Key library and the Key detail box)
                let (ly, lh) = (y0 + ui.get_mouse_low_top(), ui.get_mouse_low_height());
                k.push((ui.get_mouse_row_wx(), ly, ui.get_mouse_row_width(), lh));
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
        // (on the Library, Escape shuts its menu, then its form, then goes back
        // from a page of its own to the Library)
        if self.ui.get_place() == "library" {
            if self.games.menu.is_some() {
                self.close_menu();
                self.refresh();
                return true;
            }
            if self.games.escape() {
                self.close_menu();
                self.refresh();
                return true;
            }
        }
        // (on Home, Escape shuts the color picker open on a pad, then lets the
        // chosen pad go, and then leaves edit mode, as Done does)
        if self.ui.get_place() == "home" && self.pick.is_some() {
            self.close_picker();
            return true;
        }
        if self.ui.get_place() == "home" && self.desk.draw_tool {
            self.desk.draw_tool = false;
            self.show_desk();
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
        if self.chosen_keeps().is_none() || self.menu.is_some() || self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.pick.is_some() || self.search_menu {
            return false;
        }
        self.let_chosen_go_now();
        self.refresh();
        true
    }

    fn menu_chose(&mut self, i: usize) {
        if let Some(row) = self.until_menu {
            self.close_menu();
            let choices = Self::until_choices(row);
            if let Some((v, _)) = choices.get(i) {
                self.set_game_launch(Self::launch_key(row as i32), "until", serde_json::json!(v));
            }
            self.refresh();
            return;
        }
        if let Some(m) = self.games.menu.clone() {
            self.close_menu();
            self.games.menu = Some(m);
            self.games.chose(i);
            return;
        }
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
        if let Some(rows) = self.scene_menu.clone() {
            self.close_menu();
            match rows.get(i) {
                Some(SceneRow::Off) => self.set_scene(None, true),
                // (selecting the scene that's on turns it off, as in iCUE)
                Some(SceneRow::Scene(id)) => {
                    let id = (self.scene.as_ref() != Some(id)).then(|| id.clone());
                    self.set_scene(id, true);
                }
                Some(SceneRow::New) => self.open_scene_form(None),
                _ => {}
            }
            self.refresh();
            return;
        }
        if let Some(m) = self.scene_form_menu.clone() {
            self.close_menu();
            self.scene_form_chose(&m, i);
            self.refresh();
            return;
        }
        if let Some(which) = self.pad_menu {
            match self.pad_rows(which).1.get(i).copied().flatten() {
                Some(PadRow::Size(k)) => {
                    self.close_menu();
                    match which {
                        PadMenu::Add => self.desk.add_pad(k),
                        PadMenu::Size => {
                            let (_, _, w, h) = crate::desk::SIZES[k];
                            self.desk.set_pad_size(Some(w), Some(h));
                        }
                    }
                    self.desk_frames_run();
                }
                Some(PadRow::Draw) => {
                    self.close_menu();
                    self.desk.draw_tool = true;
                    self.desk.pad_chosen = None;
                }
                // (a heading, the rule, or a field's name: the menu stays)
                _ => return,
            }
            self.show_desk();
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
        if self.mouse.menu.is_some() {
            // (the row's job as it was offered: a heading's row has none)
            let pick = self.mouse.menu_picks.get(i).cloned().flatten();
            self.close_menu();
            if let (Some(a), Some(b)) = (pick, self.mouse.button) {
                self.mouse.set_job(b, a);
            }
            self.refresh();
            return;
        }
        if self.search_menu {
            self.close_search_menu();
            if let (Some(f), Some((plugin, setting, _))) = (self.found.0.get(i).cloned(), self.typing.take()) {
                self.set_plugin_value(&plugin, &setting, f["value"].clone());
            }
            return;
        }
        if let Some(k) = self.light_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            // (a rule, or the value no option has, after a rule of its own, changes nothing)
            if let Some((id, d, _)) = self.page.described(crate::described::Piece::Effect(k)) {
                if let Some(v) = crate::described::menu_pick(&d, i) {
                    self.page.change_settings(|e| crate::described::keep(e, &id, &d, &v));
                    self.push_preview();
                }
            }
            self.refresh();
            return;
        }
        if let Some((s, _)) = self.bind_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
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
        // New screen layout's menu: a blank layout, a copy of one, or one Ectomatter ships put
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

    /// The code a key plays playlist `id` with (see screen_keys::code_for), kept in Ectomatter's
    /// settings once the playlist has one; None while every code is some key's.
    fn play_code(&mut self, id: &str) -> Option<u32> {
        let lists = &self.screens.playlists;
        // (a code kept for one of Ectomatter's jobs is as much in use as a playlist's)
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
        // (and the codes Ectomatter's own jobs have, in their words)
        let names = self.job_names();
        let jobs: Vec<(u32, Option<crate::jobs::Job>)> = self.keyed.iter().filter_map(|(c, id)| Some((*c, crate::jobs::Job::of(id.strip_prefix(crate::jobs::ON_KEY)?)))).collect();
        self.page.job_keys = jobs.iter().map(|(c, j)| (*c, j.as_ref().map_or_else(|| "Deleted macro".to_string(), |j| names.of(j)))).collect();
        self.page.job_prints = jobs
            .iter()
            .map(|(c, j)| {
                let print = match j {
                    Some(crate::jobs::Job::Profile { device, to: crate::jobs::To::Id(id) }) => names.device(device).and_then(|d| d.profiles.iter().find(|(k, _)| k == id)).map_or_else(|| "Deleted".to_string(), |(_, n)| n.clone()),
                    Some(crate::jobs::Job::Macro { n, .. }) => names.macros.iter().find(|(k, _)| k == n).map_or_else(|| "Deleted".to_string(), |(_, n)| n.clone()),
                    Some(crate::jobs::Job::Scene(crate::jobs::SceneTo::Id(id))) => names.scenes.iter().find(|(k, _)| k == id).map_or_else(|| "Deleted".to_string(), |(_, n)| n.clone()),
                    Some(j) => names.of(j),
                    None => "Deleted".into(),
                };
                (*c, print)
            })
            .collect();
    }

    /// What a key given one of Ectomatter's own jobs (`id`, see jobs.rs) does,
    /// as the note under its job says it.
    fn job_note(&self, id: &str) -> String {
        use crate::jobs::{Job, To};
        let names = self.job_names();
        let Some(j) = Job::of(id) else {
            return "The macro this key played was deleted, so the key does nothing until you give it another job.".into();
        };
        match &j {
            Job::Profile { device, to } => match (names.device(device), to) {
                (None, _) => "The device this key switched isn’t here, so the key does nothing until it’s back.".into(),
                (Some(d), To::Id(p)) => match d.profiles.iter().find(|(k, _)| k == p) {
                    Some((_, name)) => format!("Switches {} to {name} while Ectomatter is running.", d.name),
                    None => format!("The {} profile this key switched to was deleted, so the key does nothing until you give it another job.", d.name),
                },
                (Some(d), To::Next) => format!("Switches {} to its next profile while Ectomatter is running.", d.name),
                (Some(d), To::Previous) => format!("Switches {} to its previous profile while Ectomatter is running.", d.name),
            },
            Job::Macro { .. } => format!("Ectomatter plays {} while it’s running. With Ectomatter closed, this key does nothing.", names.of(&j)),
            Job::Screen(_) => format!("{} while Ectomatter is running.", names.of(&j)),
            Job::Scene(crate::jobs::SceneTo::Id(p)) => match names.scenes.iter().find(|(k, _)| k == p) {
                Some((_, name)) => format!("Turns on the {name} scene while Ectomatter is running. Pressed again, the key turns the scene off."),
                None => "The scene this key turned on was deleted, so the key does nothing until you give it another job.".into(),
            },
            Job::Scene(crate::jobs::SceneTo::Next) => "Switches to the next scene while Ectomatter is running.".into(),
            Job::Scene(crate::jobs::SceneTo::Previous) => "Switches to the previous scene while Ectomatter is running.".into(),
            Job::Scene(crate::jobs::SceneTo::Off) => "Turns the scene off while Ectomatter is running.".into(),
        }
    }

    /// The job a key's action is, if it is one of Ectomatter's own (its host
    /// slot's code kept for one; see jobs.rs).
    fn key_job(&self, a: &Action) -> Option<crate::jobs::Job> {
        let code = z10::screen_job(a)?;
        crate::jobs::Job::of(self.keyed.get(&code)?.strip_prefix(crate::jobs::ON_KEY)?)
    }

    /// Whether a key's action is one of Ectomatter's own jobs' codes, a job
    /// deleted since included.
    fn is_job_key(&self, a: &Action) -> bool {
        z10::screen_job(a).and_then(|c| self.keyed.get(&c)).is_some_and(|id| id.starts_with(crate::jobs::ON_KEY))
    }

    /// What kind of job a key has, as its kind switch and the menu's pages
    /// tell it: one of Ectomatter's own, by what it does (another device's
    /// profile is a profile, Ectomatter's macro a macro).
    fn key_tag(&self, a: &Action) -> &'static str {
        match self.key_job(a) {
            Some(crate::jobs::Job::Profile { .. } | crate::jobs::Job::Scene(_)) => "Profile",
            Some(crate::jobs::Job::Macro { .. }) => "Macro",
            Some(crate::jobs::Job::Screen(_)) => "Screen",
            None if self.is_job_key(a) => "Macro",
            None => z10::kind_of(a),
        }
    }

    /// Other devices' jobs, Ectomatter's own macros and the scenes' jobs, as
    /// the action menu lists them: each other device's (by its name, as they
    /// sit on the desk), the macros, and the scenes'. Each one's row (its
    /// definition, for its tick, once it has a code), what choosing it does,
    /// its row's kind (5 while every code is some key's), and its words under
    /// its heading (Names::short).
    #[allow(clippy::type_complexity)]
    fn device_jobs(&self) -> (Vec<(String, Vec<(Entry, Choice, i32, String)>)>, Vec<(Entry, Choice, i32, String)>, Vec<(Entry, Choice, i32, String)>) {
        let sent = self.page.sent_codes();
        let lists = &self.screens.playlists;
        let (mut groups, mut macros, mut scenes) = (Vec::<(String, String, Vec<(Entry, Choice, i32, String)>)>::new(), Vec::new(), Vec::new());
        let names = self.job_names();
        let mut jobs = names.for_device(crate::jobs::KEYBOARD);
        jobs.extend(names.macros.iter().map(|(n, _)| crate::jobs::Job::Macro { n: *n, plays: ectomatter_protocol::Plays::Once }).map(|j| (names.of(&j), j)));
        for (words, j) in jobs {
            let short = names.short(&j);
            let id = format!("{}{}", crate::jobs::ON_KEY, j.id());
            let code = self.keyed.iter().find(|(_, k)| **k == id).map(|(c, _)| *c);
            let can = code.is_some() || crate::screen_keys::code_for(&id, &self.keyed, &sent, |p| p.starts_with(crate::jobs::ON_KEY) || lists.iter().any(|x| x.id == p)).is_some();
            // (a device's job tagged with its sort, so a search for "mouse" finds the mouse's)
            let tag = match j {
                crate::jobs::Job::Macro { .. } => "Macro",
                crate::jobs::Job::Scene(_) => "Scene",
                _ => names.sort_of(&j),
            };
            let a = Entry { label: words, act: code.map_or(Action::Raw(Vec::new()), z10::screen_slot), tag };
            let row = if can { (a, Choice::Job(j.id()), 0, short) } else { (a, Choice::Nothing, 5, short) };
            match &j {
                crate::jobs::Job::Macro { .. } => macros.push(row),
                crate::jobs::Job::Scene(_) => scenes.push(row),
                _ => {
                    let (key, head) = names.group(&j).unwrap_or_default();
                    match groups.iter_mut().find(|(k, ..)| *k == key) {
                        Some((_, _, list)) => list.push(row),
                        None => groups.push((key, head, vec![row])),
                    }
                }
            }
        }
        // (each device's by its key, two of a name kept apart; under its name)
        let profiles = groups.into_iter().map(|(_, head, list)| (head, list)).collect();
        (profiles, macros, scenes)
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
    /// reports tell of it: Record takes it as the key's job; else, while Ectomatter plays the
    /// screen, the job is done for the profile whose playlist plays. Nothing while a layout is
    /// open in the Screen Editor, the Onboard layout tab has the screen, or another program's
    /// screen shows (as a game's launch).
    /// A key with one of the keyboard's host slots pressed (`down`) or let
    /// go: one given one of Ectomatter's own jobs does it (a macro played as
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
        // (one of Ectomatter's own jobs)
        if let Some(id) = self.keyed.get(&code).and_then(|id| id.strip_prefix(crate::jobs::ON_KEY)) {
            return self.job_note(id);
        }
        let pl = &self.screens.playlist(self.page.editing).name;
        match code {
            z10::NEXT_LAYOUT => format!("Shows the next layout in {pl} while Ectomatter is running."),
            z10::PREV_LAYOUT => format!("Shows the previous layout in {pl} while Ectomatter is running."),
            z10::NEXT_PLAYLIST => format!("Plays the playlist after {pl} while Ectomatter is running."),
            z10::PREV_PLAYLIST => format!("Plays the playlist before {pl} while Ectomatter is running."),
            c => match self.page.playlist_keys.get(&c) {
                Some(name) => format!("Plays {name} while Ectomatter is running."),
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
        let row = |kind: i32, label: &str, tag: &str, tick: bool| MenuRow { kind, label: label.into(), tag: tag.into(), tick, sub: "".into(), ..Default::default() };
        let q = m.query.trim().to_lowercase();
        // each job as a row: what choosing it does, and the row's kind (0, or 5 for one that
        // can't be given now)
        let set = |list: &[Entry]| list.iter().map(|e| (e.clone(), Choice::Set(e.act.clone()), 0)).collect::<Vec<_>>();
        let plays = self.play_jobs();
        let (by_device, own_macros, scene_jobs) = self.device_jobs();
        // (in whole words, or by their own names under their heading)
        let whole = |v: &[(Entry, Choice, i32, String)]| v.iter().map(|(e, c, k, _)| (e.clone(), c.clone(), *k)).collect::<Vec<_>>();
        let headed = |v: &[(Entry, Choice, i32, String)]| v.iter().map(|(e, c, k, s)| (Entry { label: s.clone(), ..e.clone() }, c.clone(), *k)).collect::<Vec<_>>();
        let (by_device_h, scene_jobs_h): (Vec<(String, Vec<(Entry, Choice, i32)>)>, _) = (by_device.iter().map(|(h, v)| (h.clone(), headed(v))).collect(), headed(&scene_jobs));
        let others: Vec<(Entry, Choice, i32)> = by_device.iter().flat_map(|(_, v)| whole(v)).collect();
        let (own_macros, scene_jobs) = (whole(&own_macros), whole(&scene_jobs));
        // (the jobs Ectomatter does for a key share the keyboard's seven spare codes with
        // the playlists: once keys use all seven, a page with a row that can't be given says why)
        let full = "The keyboard has room for seven of Ectomatter’s jobs, playlists included, and keys on your profiles fill all seven. To make room, give the keys that have one of those jobs something else to do.";
        let job_key = self.is_job_key(&def);
        let def_tag = self.key_tag(&def);
        // a searchable list, as pickList draws it
        let pick = |rows: &mut Vec<MenuRow>, choices: &mut Vec<Choice>, groups: &[(&str, Vec<(Entry, Choice, i32)>)]| {
            let mut n = 0;
            for (head, items) in groups {
                // (a group's heading found finds all of it: "steelseries" lists the mouse's,
                // and "mouse" finds its rows by their tag)
                let whole_group = !q.is_empty() && !head.is_empty() && head.to_lowercase().contains(&q);
                let hit: Vec<&(Entry, Choice, i32)> = items.iter().filter(|(a, ..)| q.is_empty() || whole_group || a.label.to_lowercase().contains(&q) || a.tag.to_lowercase().contains(&q)).collect();
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
                    // (the keyboard's own, then those Ectomatter plays)
                    pick(&mut rows, &mut choices, &[("Macros", set(&g.macros)), ("Ectomatter's macros", own_macros.clone())]);
                    if q.is_empty() && own_macros.iter().any(|(.., kind)| *kind == 5) {
                        rows.push(row(3, full, "", false));
                        choices.push(Choice::Nothing);
                    }
                }
            }
            Some(p) => {
                let title = PAGES[p];
                let what = title.trim_start_matches("A ").trim_start_matches("An ").to_lowercase();
                // (the profiles' page holds the scenes too, and says so)
                placeholder = if p == PROFILE_PAGE { "Search profiles and scenes".to_string() } else { format!("Search {what}s") };
                // (the playlists' own jobs after stepping through them; other devices'
                // profiles after the keyboard's, under the devices' names)
                let mut items = set(lists[p]);
                if p == PLAYLIST_PAGE {
                    items.extend(plays.iter().cloned());
                }
                if p == PROFILE_PAGE {
                    // (each device's profiles and the scenes under their headings, each by its own
                    // name, as the Macro Editor lists them; a search across every page has them whole)
                    let own: Vec<(Entry, Choice, i32)> = items
                        .iter()
                        .map(|(e, c, k)| {
                            let label = match &e.act {
                                Action::Profile(ectomatter_protocol::Jump::To(n)) => self.page.profile_name(*n as u8),
                                _ => e.label.clone(),
                            };
                            (Entry { label, ..e.clone() }, c.clone(), *k)
                        })
                        .collect();
                    // (this keyboard's own, then each other device's, then the scenes, each under its name)
                    let mine = crate::cores::shown().name().to_string();
                    let mut groups: Vec<(&str, Vec<(Entry, Choice, i32)>)> = vec![(mine.as_str(), own)];
                    groups.extend(by_device_h.iter().map(|(h, v)| (h.as_str(), v.clone())));
                    groups.push(("Scenes", scene_jobs_h.clone()));
                    pick(&mut rows, &mut choices, &groups);
                } else {
                    pick(&mut rows, &mut choices, &[(title, items)]);
                }
                let greyed = match p {
                    PLAYLIST_PAGE => plays.iter().any(|(.., kind)| *kind == 5),
                    PROFILE_PAGE => others.iter().chain(&scene_jobs).any(|(.., kind)| *kind == 5),
                    _ => false,
                };
                if q.is_empty() && greyed {
                    rows.push(row(3, full, "", false));
                    choices.push(Choice::Nothing);
                }
            }
            None => {
                placeholder = "Search keys, media, macros".to_string();
                if !q.is_empty() {
                    let mut all = set(&g.macros.iter().chain(&g.keys).chain(&g.media).chain(&g.mouse).chain(&g.profiles).chain(&g.layouts).chain(&g.playlists).chain(&g.board).cloned().collect::<Vec<_>>());
                    all.extend(plays.iter().cloned());
                    all.extend(others.iter().cloned());
                    all.extend(scene_jobs.iter().cloned());
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
                    // (Ectomatter's own macros after the keyboard's)
                    if !own_macros.is_empty() {
                        rows.push(row(1, "Ectomatter's macros", "", false));
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
                        let title = if p == PROFILE_PAGE { "Switch profile or scene" } else { title };
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
        // (its notes wrapped to the width it opened at)
        crate::menu_size::fit_notes(&mut rows, self.ui.get_menu_w());
        self.ui.set_menu_natural(crate::menu_size::height(&rows, true, back, f32::MAX));
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        if let Some(m) = &mut self.menu {
            m.choices = choices;
        }
    }

    // ------------------------------------------------------------ the page's models

    /// Draw the page from the page's state.
    pub fn refresh(&mut self) {
        // (the page's sensors read only while Settings or the screen editor shows them)
        if self.ui.get_place() != "settings" && self.layouts.is_none() {
            if let Some(r) = &mut self.reader {
                r.rest();
            }
        }
        self.note_changes();
        self.sync_playlist_keys();
        self.follow_profile();
        self.sync_aside();
        // (a scene that lights the keyboard says so in place of the Lighting page's hint)
        let kb_scene = self.scene_lighting("z10");
        let lighting_hint = match &kb_scene {
            Some(name) => format!("The {name} scene is lighting the desk. Choosing an effect here turns the scene off."),
            None => "Choose a lighting effect for this profile, then set the colors, speed and zones below.".to_string(),
        };
        if kb_scene.is_some() {
            self.ui.set_hint_chip(self.scene_chip(self.scene_on().as_ref()));
        }
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
        ui.set_hint_chip_on(p.view == View::Lighting && kb_scene.is_some());
        ui.set_hint(
            match p.view {
                View::Keys if p.sel.is_some() => "Escape clears the choice.",
                View::Keys => "Select a key on the keyboard or in the Key library to change what a key does.",
                View::Lighting => lighting_hint.as_str(),
                View::Display => "Check the keyboard’s connection and which features the installed firmware supports.",
                View::Screen => "Choose what the keyboard’s screen shows while Ectomatter is running, when a game starts or quits, and when Ectomatter isn’t running.",
            }
            .into(),
        );
        // on Screen, what the screen is doing now, beside Pause
        ui.set_screen_status(self.screen_status().into());

        // the keys over the keyboard; on Lighting, the area the pointer is over outlined
        let (x0, y0, _, _) = p.view_box;
        let zoned = self.area_keys();
        let some = self.some_areas();
        // (painting key by key: a click on a lit key paints it)
        let painting = p.drafts.contains_key(&p.editing) && p.effect_desc(&p.effect_id())["paint"] == true;
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
                    View::Lighting if painting => k.led,
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
            // something newly chosen: the list scrolls to its row (a group's heading, then its
            // rows, each a control's height)
            if newly && chosen.is_some() {
                let mut y = 0.0;
                for g in &groups {
                    y += crate::tokens::size::LIST_HEAD;
                    let rows: Vec<LibRow> = (0..g.rows.row_count()).filter_map(|i| g.rows.row_data(i)).collect();
                    if let Some(i) = rows.iter().position(|r| r.chosen) {
                        ui.set_lib_reveal_y(y + i as f32 * crate::tokens::size::CONTROL);
                        ui.set_lib_reveal_n(ui.get_lib_reveal_n() + 1);
                        break;
                    }
                    y += rows.len() as f32 * crate::tokens::size::CONTROL;
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
        self.take_old_ties();
        if self.ui.get_place() == "mouse" {
            self.refresh_mouse();
        } else if self.ui.get_place() == "library" {
            self.show_library();
        } else if self.ui.get_place() == "settings" {
            // what the kernel says of the keyboard, looked at every two seconds (it
            // scans every process), not at every step of a slider
            let sensors = self.sensors_view();
            self.hosted.poll();
            let st = crate::ecto_settings::State {
                sensors: Some(sensors),
                devices: self.device_rows(),
                plugins: self.package_rows(crate::packages::Kind::Plugin),
                sections: self.plugin_sections(),
                settings: self.settings.all(),
                top_peak: self.levels.1,
                reference: self.levels.0,
            };
            let (cells, h, map, pmap) = crate::ecto_settings::page(&st, self.ui.get_content_width());
            self.sensor_map = map;
            self.plugin_map = pmap;
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
                let names = ["Playlist", "Onboard layout", "Game launch"];
                let tabs: f32 = names.iter().map(|t| crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CONTROL, t) + 36.0).sum::<f32>() + 4.0 * (names.len() - 1) as f32;
                crate::display::screen_cells(&self.page, w, tabs + 10.0 + 24.0, own.as_ref().map(|(n, l)| (n.as_str(), *l)), picking)
            };
            self.show_cells(cells);
            self.refresh_onboard();
            self.show_game_rows();
            self.refresh_editor();
            self.draw_screen();
        } else if loaded && self.page.view == View::Display {
            let fw = self.firmware();
            let connected = match self.ui.get_conn_state() {
                1 => Some(true),
                3 => None,
                _ => Some(false),
            };
            let t = crate::display::tiles(&fw, connected, self.ui.get_page_w());
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

    /// Where Ectomatter's macros run, for the Macro Editor's library of them:
    /// each mouse profile's buttons given one, and how each plays there.
    pub fn macros_elsewhere(&mut self) {
        // (the words the steps that do a job say it by: the devices' profiles, the macros)
        let jobs = self.job_names();
        let Some(m) = &mut self.macros else { return };
        m.mouse_holds = self.mouse.profiles.is_some();
        m.ed.jobs = jobs.clone();
        m.other.jobs = jobs;
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
        // the keyboard's keys given one, by the codes Ectomatter keeps for its jobs, on its
        // profiles (named after the mouse's)
        if self.page.dev.is_some() {
            let first = names.len();
            for (k, &p) in z10::PROFILE_NUMBERS.iter().enumerate() {
                names.push(format!("keyboard’s {}", self.page.profile_name(p)));
                let Some(d) = self.page.drafts.get(&p) else { continue };
                for (layer, words) in [("base", ""), ("fn", "Fn + ")] {
                    for pos in 0..evga_z10::protocol::NKEYS as u32 {
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
            if ed.lib == crate::macros::Lib::Ectomatter {
                ed.elsewhere = runs.clone();
                ed.names = names.clone();
            }
        }
    }

    /// How one of Ectomatter's macros plays, set in the Macro Editor, on every
    /// mouse button that runs it, on every profile.
    pub fn macro_plays(&mut self, n: u32, how: ectomatter_protocol::Plays) {
        // (the keyboard's keys that run it, kept against their codes)
        let job = format!("{}{}", crate::jobs::ON_KEY, crate::jobs::Job::Macro { n, plays: how }.id());
        self.rekey(|id| (matches!(crate::jobs::Job::of(id.strip_prefix(crate::jobs::ON_KEY)?), Some(crate::jobs::Job::Macro { n: m, .. }) if m == n)).then(|| job.clone()));
        let Some(store) = &mut self.mouse.profiles else { return };
        let job = Action::Macro { n, plays: how }.write();
        // (the profile in use is on the mouse: a button that holds the macro
        // itself, or stops being able to, is told)
        for bi in crate::mouse::rewrite_macro(store, n, &job) {
            self.mouse.set_job(bi, job.clone());
        }
        self.macros_elsewhere();
        self.refresh();
    }

    /// One of Ectomatter's macros saved with new steps: each of the mouse's
    /// buttons that runs it told again, for the mouse to hold the macro as it
    /// is now (or to leave it to Ectomatter, if it can't hold it any more).
    pub fn macro_saved(&mut self, n: u32) {
        for bi in 0..crate::mouse::BUTTONS.len() {
            let job = self.mouse.job(bi);
            if matches!(Action::read(&job), Some(Action::Macro { n: m, .. }) if m == n) {
                self.mouse.set_job(bi, job);
            }
        }
        self.macros_elsewhere();
        self.refresh();
    }

    /// Ectomatter's jobs kept against the keyboard's codes, each one `f` gives
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

    /// One of Ectomatter's macros deleted: every mouse button that ran it, on
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

    /// A control given a job Ectomatter does (`job`), pressed or let go: one of
    /// Ectomatter's macros played, as its job says it plays.
    fn play_for(&mut self, control: &str, job: Option<serde_json::Value>, down: bool) {
        let Some(j) = job.as_ref().and_then(crate::jobs::Job::read) else { return };
        let crate::jobs::Job::Macro { n, plays } = j else {
            self.run_job(&j, down);
            return;
        };
        let jobs = self.macro_jobs.clone();
        let player = self.typist.get_or_insert_with(|| crate::typist::Player::start(jobs));
        if !down {
            player.up(control);
            return;
        }
        // (the macros it plays put in their places: the player plays keys, waits and jobs)
        if let Some(m) = crate::library::get(n) {
            let steps = crate::macros::flatten(m["steps"].as_array().map_or(&[][..], |v| v.as_slice()), &mut vec![n]);
            player.down(control, steps, plays);
        }
    }

    /// A job one of Ectomatter's macros does as a step, by its id, as its
    /// player reaches it: done as a control given the job would do it.
    pub fn macro_job(&mut self, id: &str) {
        if let Some(j) = crate::jobs::Job::of(id).filter(|j| !matches!(j, crate::jobs::Job::Macro { .. })) {
            self.run_job(&j, true);
        }
    }

    /// One of Ectomatter's own jobs, its control pressed (`down`) or let go:
    /// another device's profile switched, one of the keyboard's screen jobs
    /// done, or the scene switched, as the control goes down. (A macro plays
    /// in play_for.)
    fn run_job(&mut self, j: &crate::jobs::Job, down: bool) {
        use crate::jobs::{Job, To};
        if !down {
            return;
        }
        eprintln!("ectomatter: a job: {}", self.job_names().of(j));
        match j {
            // (each device's profiles switched as its page switches them; a plugin's device
            // through its plugin)
            Job::Profile { device, to } if device == crate::jobs::KEYBOARD => {
                if self.page.dev.is_none() {
                    return;
                }
                let now = self.page.active() as usize;
                let n = match to {
                    To::Id(id) => match id.parse::<usize>() {
                        Ok(n) => n,
                        Err(_) => return,
                    },
                    To::Next => z10::PROFILE_NUMBERS[crate::jobs::stepped(now.saturating_sub(1), z10::PROFILE_NUMBERS.len(), true)] as usize,
                    To::Previous => z10::PROFILE_NUMBERS[crate::jobs::stepped(now.saturating_sub(1), z10::PROFILE_NUMBERS.len(), false)] as usize,
                };
                if z10::PROFILE_NUMBERS.contains(&(n as u8)) && n != now {
                    self.choose_profile(n as u8);
                    self.refresh();
                }
            }
            Job::Profile { device, to } if device == crate::jobs::MOUSE => {
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
            Job::Profile { device, to: To::Id(p) } => self.plugin_choose(device, p),
            // (a plugin's device steps through its profiles only as its plugin can: none does yet)
            Job::Profile { device, .. } => eprintln!("ectomatter: {device} can’t step through its profiles"),
            Job::Screen(code) => self.screen_key(*code),
            Job::Scene(to) => {
                // (chosen by hand, as on Home: kept, and on again when Ectomatter next starts;
                // none once Ectomatter is quitting, which the desk would never show)
                if self.scene_quit_done {
                    return;
                }
                let ids: Vec<String> = self.scenes().into_iter().map(|s| s.id).collect();
                let Some(want) = crate::jobs::scene_after(to, self.scene.as_deref(), &ids) else { return };
                self.set_scene(want, true);
                self.refresh();
            }
            Job::Macro { .. } => {}
        }
    }

    /// The names other devices' jobs are said by, as the pages have them now:
    /// every device with profiles a control can switch, as the Library lists
    /// them (the keyboard once it's read; a plugin's device that can be
    /// switched by hand), Ectomatter's macros, the scenes.
    pub fn job_names(&self) -> crate::jobs::Names {
        let mut devices: Vec<crate::jobs::Profiled> = self
            .lib_devices()
            .into_iter()
            .map(|d| crate::jobs::Profiled { steps: true, screen: d.key == crate::jobs::KEYBOARD, here: d.key != crate::jobs::KEYBOARD || self.page.dev.is_some(), key: d.key, name: d.name, kind: d.kind, profiles: d.profiles })
            .collect();
        for (_, d) in self.hosted.devices() {
            let cap = &d["capabilities"]["profiles"];
            let key = d["device"].as_str().unwrap_or("").to_string();
            // (a device id a job's id can carry, and no other device's)
            if cap["choose"] != true || key.is_empty() || key.contains(':') || devices.iter().any(|x| x.key == key) {
                continue;
            }
            let profiles = cap["list"].as_array().into_iter().flatten().filter_map(|p| Some((p["id"].as_str()?.to_string(), p["name"].as_str().unwrap_or("").to_string()))).collect();
            devices.push(crate::jobs::Profiled { key, name: d["name"].as_str().unwrap_or("").into(), kind: d["kind"].as_str().unwrap_or("").into(), profiles, steps: false, screen: false, here: true });
        }
        let all = crate::library::all();
        let mut macros: Vec<(u32, String)> = all.as_object().map(|m| m.iter().filter_map(|(n, v)| Some((n.parse().ok()?, v["name"].as_str().filter(|s| !s.is_empty()).map_or_else(|| format!("Macro {n}"), String::from)))).collect()).unwrap_or_default();
        macros.sort();
        let scenes = self.scenes().into_iter().map(|s| (s.id, s.name)).collect();
        crate::jobs::Names { devices, macros, scenes }
    }

    /// The devices Ectomatter shows, as they sit on Home's desk (left to right),
    /// then any not on it yet: each one's place, its name, what sort it is,
    /// and whether it is plugged in. The keyboard is always among them, the
    /// mouse once Ectomatter has met it.
    pub fn devices_in_order(&self) -> Vec<(String, String, String, bool)> {
        let kb = || ("z10".to_string(), crate::cores::shown().name().to_string(), "keyboard".to_string(), self.ui.get_conn_state() == 1);
        let mouse = || {
            let name = self.mouse.state["device"]["name"].as_str().unwrap_or("SteelSeries Sensei 310").to_string();
            ("mouse".to_string(), name, "mouse".to_string(), self.mouse.present())
        };
        let mut out: Vec<(String, String, String, bool)> = Vec::new();
        for id in self.desk.order().iter().map(String::as_str).chain(["z10", "mouse"]) {
            if out.iter().any(|d| d.0 == id) {
                continue;
            }
            match id {
                "z10" => out.push(kb()),
                "mouse" if self.mouse.known() => out.push(mouse()),
                _ => {}
            }
        }
        out
    }

    /// The rail's devices, as they sit on Home's desk.
    fn show_rail(&self) {
        let ui = &self.ui;
        let rows: Vec<crate::RailDevice> = self.devices_in_order().into_iter().map(|(place, name, kind, _)| crate::RailDevice { place: place.into(), name: name.into(), kind: kind.into() }).collect();
        // (each device's page names it in its bar as the rail does)
        for r in &rows {
            match r.place.as_str() {
                "z10" if ui.get_kb_name() != r.name => ui.set_kb_name(r.name.clone()),
                "mouse" if ui.get_mouse_name() != r.name => ui.set_mouse_name(r.name.clone()),
                _ => {}
            }
        }
        show_rows(ui.get_rail_devices(), |r| ui.set_rail_devices(r), rows);
    }

    /// What the mouse's core says, from the mouse's thread.
    pub fn mouse_heard(&mut self, h: crate::mouse::Heard) {
        use crate::mouse::Heard;
        match h {
            Heard::State(v) => {
                self.mouse.heard_state(v);
                self.mouse.failed = None;
                // (a mouse gone lets every button go)
                if !self.mouse.present() && self.mouse_down != 0 {
                    self.mouse_down = 0;
                    self.show_mouse_held();
                }
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
            // a button pressed or let go: a press sends a ripple across Home's desk
            // from the button, as a key's does (but the pointer's click, whose ripple
            // comes from where it clicks the desk); one given one of Ectomatter's
            // macros plays it (let go ends one that plays while it's held)
            Heard::Input(control, down) => {
                // (held down on its pictures as it is on the mouse)
                if let Some(b) = crate::mouse::BUTTONS.iter().position(|(id, ..)| *id == control) {
                    let held = if down { self.mouse_down | 1 << b } else { self.mouse_down & !(1 << b) };
                    if held != self.mouse_down {
                        self.mouse_down = held;
                        self.show_mouse_held();
                    }
                }
                if down {
                    self.ripple_from_button(&control, Duration::ZERO);
                }
                self.play_for(&format!("mouse/{control}"), crate::mouse::BUTTONS.iter().position(|(b, ..)| *b == control).map(|i| self.mouse.job(i)), down);
                return;
            }
        }
        self.ui.set_mouse_known(self.mouse.known());
        self.show_rail();
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
            // (ECTOMATTER_MOUSE_ART: another copy of the drawing, to try a change to it offscreen)
            let path = std::env::var_os("ECTOMATTER_MOUSE_ART").map(std::path::PathBuf::from).unwrap_or_else(|| crate::cores::dir().join(crate::mouse::CORE).join("art/sensei-310.svg"));
            let art = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            crate::mouse_art::layers(&art, w, &crate::stage_shadow(scale), crate::STAGE_SHADOW_OPACITY)
        };
        match self.mouse.tell.clone() {
            Some(tell) => {
                std::thread::spawn(move || match draw() {
                    Ok(l) => {
                        let _ = tell.send(crate::mouse::Heard::Drawn(w, scale, Box::new(l)));
                    }
                    Err(e) => eprintln!("ectomatter: the mouse's picture: {e}"),
                });
            }
            None => match draw() {
                Ok(l) => self.mouse_drawn(w, scale, l),
                Err(e) => eprintln!("ectomatter: the mouse's picture: {e}"),
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
            self.light_desk_mouse();
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
            self.light_desk_mouse();
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

    /// The mouse's buttons held down, as they are now, on the picture of it
    /// that shows (drawn again now), and on the other when it next shows;
    /// neither drawn while the window is closed into the tray.
    fn show_mouse_held(&mut self) {
        let shows = !self.hidden && self.mouse_pictured();
        let place = self.ui.get_place();
        if shows && place == "home" {
            self.light_desk_mouse();
        } else {
            self.desk_mouse_lit = None;
        }
        if shows && place == "mouse" {
            self.mouse_light_up();
        } else {
            self.mouse_lit = None;
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

    /// How the mouse's lights are made: as the lab tunes them, or as its
    /// core's light file has them.
    pub fn mouse_light_model(&self) -> serde_json::Value {
        self.mouse_light.clone().unwrap_or_else(|| crate::mouse::light_model(&self.mouse.state["device"]))
    }

    /// The mouse lit as the lab tunes it: its lamps made again for that.
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
    fn light_desk_mouse(&mut self) {
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
        let lit = hdr.on_ground(layers, colors, if on { 1.0 } else { 0.5 }, self.mouse_down, white, ceiling);
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

    /// The mouse's lights as the lab chooses them, one at a time: each
    /// where its window is on the drawing, by its id and name. None before
    /// the mouse's picture is drawn.
    pub fn mouse_lab_parts(&self) -> Option<Vec<ectomatter_draw::hdr::CapShape>> {
        let (scale, imgs) = self.mouse_layers.as_ref()?;
        let boxes = crate::mouse_art::light_boxes(&imgs.layers, (240.0 * scale).round() as u32);
        // (in the core's order, as the lights are on the mouse)
        Some(
            self.mouse
                .light_order()
                .into_iter()
                .filter_map(|(i, id, label)| {
                    let b = *boxes.get(crate::mouse_art::LIGHT_IDS.iter().position(|l| *l == id)?)?;
                    let _ = i;
                    Some(ectomatter_draw::hdr::CapShape { id: id.to_string(), label, short: None, foot: b, lit: true, legend: false, button: false, face: b, print: None, bump: Default::default() })
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
        let lit = hdr.picture(&imgs.layers, crate::ink_env::rgb("stage", crate::tokens::ink::STAGE), self.mouse_shows.0, self.mouse_shows.1, self.mouse_down, colors, white, ceiling, part);
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
                self.light_desk_mouse();
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

    /// The devices Ectomatter knows, on the desk: those it has read or met, and
    /// those it has kept a place for; each at its size, and whether it is
    /// plugged in.
    fn desk_meet(&mut self) {
        if self.page.dev.is_some() || self.desk.keeps("z10") {
            let core = crate::cores::shown();
            let size = crate::desk::size_of(&core.art, core.device["light"]["scale"].as_f64().unwrap_or(1.38) as f32);
            // (not yet known to be plugged in, it is drawn as plugged in: only a keyboard gone shows dimmed)
            let kind = core.device["kind"].as_str().unwrap_or("keyboard").to_string();
            self.desk.meet("z10", core.name(), "z10", &kind, size, self.ui.get_conn_state() != 2, true);
        }
        if self.mouse.known() || self.desk.keeps("mouse") {
            let dev = &self.mouse.state["device"];
            let v = crate::mouse_art::VIEW;
            let per = dev["scale"].as_f64().unwrap_or(520.0 / 125.1) as f32;
            let size = (v.2 / per * crate::desk::PER_MM, v.3 / per * crate::desk::PER_MM);
            let name = dev["name"].as_str().unwrap_or("SteelSeries Sensei 310").to_string();
            let kind = dev["kind"].as_str().unwrap_or("mouse").to_string();
            self.desk.meet("mouse", &name, "mouse", &kind, size, self.mouse.present() || self.mouse.state.is_null(), false);
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
        ui.set_desk_pad_size(chosen.map_or(String::new(), |p| crate::desk::size_words(p.w / crate::desk::PER_MM, p.h / crate::desk::PER_MM)).into());
        ui.set_desk_drawing(d.editing && d.draw_tool);
        ui.set_desk_pad_note(self.pad_note.as_str().into());
        ui.set_desk_editing(d.editing);
        self.show_rail();
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

    /// The desk's easing, fading and ripples, a frame each sixtieth of a
    /// second while they go on.
    fn desk_frames_run(&mut self) {
        let want = self.desk.animating() || self.desk.rippling();
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
                let nw = crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::TEXT, &it.name) + 15.0;
                let sw = if it.connected { 0.0 } else { crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, "Not connected") };
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
                eprintln!("ectomatter: a pad's picture, {path}: {e}");
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
    /// (offscreen, ECTOMATTER_PAD_PICTURE's file, as if chosen there).
    fn ask_pad_picture(&mut self) {
        let Some((i, b)) = self.desk.chosen_box() else { return };
        if matches!(self.pad_asked, Some((_, _, PadAsk::Picking(_)))) {
            return;
        }
        self.close_picker();
        self.pad_note.clear();
        let rx = match std::env::var_os("ECTOMATTER_PAD_PICTURE").filter(|_| crate::desk::offscreen()) {
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
        let mouse_scene = (self.mouse.tab == 1).then(|| self.scene_lighting("sensei310")).flatten();
        if mouse_scene.is_some() {
            self.ui.set_mouse_hint_chip(self.scene_chip(self.scene_on().as_ref()));
        }
        let ui = &self.ui;
        let m = &self.mouse;
        ui.set_mouse_tab(m.tab as i32);
        ui.set_mouse_hint_chip_on(mouse_scene.is_some());
        match &mouse_scene {
            Some(name) => ui.set_mouse_hint(format!("The {name} scene is lighting the desk. Choosing an effect here turns the scene off.").into()),
            None => ui.set_mouse_hint(m.hint().into()),
        }
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
        // Buttons: the Key library, as the Keys tab has it: the layer the page
        // edits as its tab, then the buttons in their groups
        let layers: Vec<LayerTab> = m.state["device"]["capabilities"]["keys"]["layers"]
            .as_array()
            .and_then(|ls| ls.first())
            .map(|l| vec![LayerTab { name: l["label"].as_str().unwrap_or("When pressed").into(), cap: "".into() }])
            .unwrap_or_else(|| vec![LayerTab { name: "When pressed".into(), cap: "".into() }]);
        show_rows(ui.get_mouse_layers(), |r| ui.set_mouse_layers(r), layers);
        let q = self.mouse_lib_query.trim().to_lowercase();
        let all = m.lib_list("");
        let list = m.lib_list(&q);
        // (its groups start open for a mouse's few buttons, scrolling in their
        // box, and shut where the whole list runs past 600 high, as the
        // keyboard's 116 keys do: a heading 30 high and a row 34, under the tab
        // and the find box)
        let need = 50.0 + 60.0 + 10.0 + all.iter().map(|(_, n, _)| 30.0 + 34.0 * *n as f32).sum::<f32>();
        if self.mouse_lib_shut.is_none() && !all.is_empty() {
            self.mouse_lib_shut = Some(if need > 600.0 { all.iter().map(|(g, ..)| g.clone()).collect() } else { Default::default() });
        }
        // a button newly chosen (on the picture, say): its group opens to show it, once
        let newly = m.button.is_some() && m.button != self.mouse_lib_revealed;
        self.mouse_lib_revealed = m.button;
        if newly {
            if let (Some(shut), Some((g, ..))) = (self.mouse_lib_shut.as_mut(), all.iter().find(|(_, _, rows)| rows.iter().any(|r| Some(r.0) == m.button))) {
                shut.remove(g);
            }
        }
        let shut = self.mouse_lib_shut.clone().unwrap_or_default();
        let mut groups: Vec<LibGroup> = Vec::new();
        for (g, total, rows) in list {
            if rows.is_empty() && !q.is_empty() {
                continue;
            }
            let open = !q.is_empty() || !shut.contains(&g);
            let rows: Vec<LibRow> = rows
                .into_iter()
                .map(|(i, cap, name, job, rebound)| LibRow { id: i as i32, cap: cap.into(), name: name.into(), job: job.into(), rebound, chosen: m.button == Some(i) })
                .collect();
            let rebound = rows.iter().filter(|r| r.rebound).count() as i32;
            let model = self.mouse_lib_rows.entry(g.clone()).or_insert_with(|| Rc::new(VecModel::default())).clone();
            set_rows(&model, if open { rows } else { Vec::new() });
            groups.push(LibGroup { name: g.into(), count: total as i32, rebound, open, rows: ModelRc::from(model) });
        }
        // (the list scrolled to it, where it is longer than its box)
        if newly {
            let mut y = 0.0;
            for g in &groups {
                y += crate::tokens::size::LIST_HEAD;
                let rows: Vec<LibRow> = (0..g.rows.row_count()).filter_map(|i| g.rows.row_data(i)).collect();
                if let Some(i) = rows.iter().position(|r| r.chosen) {
                    ui.set_mouse_lib_reveal_y(y + i as f32 * crate::tokens::size::CONTROL);
                    ui.set_mouse_lib_reveal_n(ui.get_mouse_lib_reveal_n() + 1);
                    break;
                }
                y += rows.len() as f32 * crate::tokens::size::CONTROL;
            }
        }
        show_rows(ui.get_mouse_lib_groups(), |r| ui.set_mouse_lib_groups(r), groups);
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
        let wide = pic_wide(self.ui.get_mouse_fx_w());
        if self.mouse_icons.as_ref().map(|(s, w, _)| (*s, *w)) != Some((self.scale, wide)) {
            let icons = ["off", "static", "breathing", "wave"].iter().map(|e| lighting::icon(e, self.scale, wide)).collect();
            self.mouse_icons = Some((self.scale, wide, icons));
        }
        let fx = self.mouse.effect();
        let known = self.mouse.light_known(self.mouse.light_name());
        let effects: Vec<EffectItem> = self
            .mouse
            .effects()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (tile, motion) = crate::mouse::tile_of(e["id"].as_str().unwrap_or(""));
                let icon = self.mouse_icons.as_ref().unwrap().2[["off", "static", "breathing", "wave"].iter().position(|t| *t == tile).unwrap_or(1)].clone();
                EffectItem { name: e["label"].as_str().unwrap_or("").into(), icon, on: i == fx && known, motion }
            })
            .collect();
        self.ui.set_mouse_effect_moves(effects.iter().any(|e| e.on && e.motion != 0));
        show_rows(self.ui.get_mouse_effects(), |r| self.ui.set_mouse_effects(r), effects);
        // (the color the picker is open on, ringed)
        let open = self.pick.as_ref().and_then(|p| match p.piece {
            Some(crate::described::Piece::Effect(k)) => Some((k, p.index)),
            _ => None,
        });
        // the tab's boxes, in the page's row of boxes, each laid out to the width the row made it;
        // on Settings, the form across the page
        let boxes = self.mouse.boxes(|i, least| self.mouse_boxes.width(i, least), open);
        self.mouse_boxes.show(boxes);
        if self.mouse.tab == 3 {
            let (cells, h) = self.mouse.cells(self.ui.get_mouse_w());
            self.ui.set_mouse_cells_h(h);
            self.show_cells(cells);
        }
        // the picture: its layers, the buttons it picks out, its lights now, and their effects playing
        self.mouse_draw();
        self.show_mouse_picked();
        self.show_mouse_lights();
        self.mouse_frames_run();
    }

    /// The menu on the mouse's profile in use: Duplicate, and Delete, which
    /// the only profile can't be.
    fn open_profile_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let only = self.mouse.profiles.as_ref().is_none_or(|p| p.list.len() < 2);
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
        let mut rows = vec![row(0, "Duplicate"), row(0, "Use with an app…"), row(2, ""), row(if only { 5 } else { 4 }, "Delete")];
        if only {
            rows.push(row(3, "Your only profile can’t be deleted."));
        }
        self.hang_menu(rows, 0.0, anchor);
        self.profile_menu = true;
    }

    /// Whether a press off the open menu shut it a moment ago, on what opens this one (so
    /// the press on its button doesn't open it again).
    fn just_shut(&mut self, anchor: (f32, f32, f32)) -> bool {
        self.shut.take().is_some_and(|(at, when)| (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800))
    }

    /// A menu of `rows` open from `anchor` (what opened it: its left, its top and its
    /// height; for one opened at the pointer, the point), as every menu opens
    /// (src/menu_size.rs): as wide as its rows need and never narrower than `under`, the
    /// field it drops from (none for a button's); under what opened it where it fits, over
    /// it where there's more room there.
    fn hang_menu(&mut self, rows: Vec<MenuRow>, under: f32, anchor: (f32, f32, f32)) {
        self.hang_menu_most(rows, under, anchor, f32::MAX);
    }

    /// hang_menu, showing at most `most` of its rows' height before they scroll.
    fn hang_menu_most(&mut self, mut rows: Vec<MenuRow>, under: f32, anchor: (f32, f32, f32), most: f32) {
        let w = crate::menu_size::width(&rows, self.ui.get_menu_marks(), under);
        crate::menu_size::fit_notes(&mut rows, w);
        let h = crate::menu_size::height(&rows, false, false, most);
        let (ax, ay, ah) = anchor;
        let win = self.ui.get_window_height();
        let (top, most) = crate::menu_size::hang(win, (ay, ah), h);
        // (turned to what's chosen, where that's out of view)
        let reveal = crate::menu_size::reveal(&rows, h.min(most) - 2.0 * crate::tokens::part::MENU_PAD);
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        self.ui.set_menu_w(w);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_ay(ay);
        self.ui.set_menu_ah(ah);
        self.ui.set_menu_top(top);
        self.ui.set_menu_max(most);
        self.ui.set_menu_open(true);
        self.ui.invoke_scroll_menu(reveal);
    }

    /// The menu on the keyboard's profile being edited: Use with an app.
    fn open_kb_profile_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
        self.hang_menu(vec![row(0, "Use with an app…")], 0.0, anchor);
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
        let mut rows = vec![MenuRow { kind: 1, label: "Use it while one of these is in front".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() }];
        rows.extend(apps.iter().map(|(id, name)| MenuRow { kind: 0, label: name.as_str().into(), tag: "".into(), tick: tied.contains(id), sub: "".into(), ..Default::default() }));
        if apps.is_empty() {
            rows.push(MenuRow { kind: 3, label: "Apps show here once they’ve been in front while Ectomatter is running.".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() });
        }
        self.hang_menu(rows, 0.0, anchor);
        self.apps_menu = Some((device, profile, anchor));
    }

    /// A device's key in the Library (`device` 0 the keyboard, 1 the mouse),
    /// and a profile's id there: the keyboard's by its number, the mouse's by
    /// the id Ectomatter gave it (from its place in the list).
    fn tie_key(&self, device: usize, profile: usize) -> Option<(&'static str, String)> {
        if device == 1 {
            return self.mouse.profiles.as_ref().and_then(|s| s.list.get(profile)).map(|p| ("sensei310", p.id.clone()));
        }
        Some(("z10", profile.to_string()))
    }

    /// The games and apps a profile is chosen for, as the Library keeps them
    /// (the keyboard's, `device` 0, by its number; the mouse's, 1, by its place).
    fn tied_apps(&self, device: usize, profile: usize) -> Vec<String> {
        self.tie_key(device, profile).map(|(d, p)| self.games.library.tied(d, &p)).unwrap_or_default()
    }

    /// A profile tied to an app, or no longer: one profile of a device at a
    /// time for each app, the last it was tied to, kept in the Library with
    /// what each game switches.
    fn tie_app(&mut self, device: usize, profile: usize, app: &str, on: bool) {
        let Some((d, p)) = self.tie_key(device, profile) else { return };
        let mine = self.games.library.choice(app, d) == Some(crate::games::Choice::Profile(p.clone()));
        if on {
            self.games.library.set_choice(app, d, Some(crate::games::Choice::Profile(p)));
        } else if mine {
            self.games.library.set_choice(app, d, None);
        }
    }

    /// The words under a profile's name for the apps it's tied to: "With
    /// Steam", or with how many more.
    fn with_apps(&self, apps: &[String]) -> Option<String> {
        let first = self.games.name(apps.first()?, &self.front_apps);
        Some(match apps.len() {
            1 => format!("With {first}"),
            n => format!("With {first} and {} more", n - 1),
        })
    }

    /// Ties made before the Library kept them, taken in once and taken out of
    /// where they were: the keyboard's from Ectomatter's settings, the mouse's
    /// from its profiles' file (once the mouse's profiles have been read).
    fn take_old_ties(&mut self) {
        if !self.ties_taken[0] {
            self.ties_taken[0] = true;
            if let Some(all) = self.settings.get("devices.z10.profile_apps").and_then(|v| v.as_object().cloned()) {
                let ties: Vec<(String, Vec<String>)> = all.iter().map(|(n, a)| (n.clone(), a.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect())).collect();
                self.games.library.take_ties("z10", &ties);
                let _ = self.settings.set("devices.z10.profile_apps", serde_json::Value::Null);
            }
        }
        if !self.ties_taken[1] {
            if let Some(store) = &mut self.mouse.profiles {
                self.ties_taken[1] = true;
                let ties: Vec<(String, Vec<String>)> = store.list.iter().filter(|p| !p.apps.is_empty()).map(|p| (p.id.clone(), p.apps.clone())).collect();
                if !ties.is_empty() {
                    self.games.library.take_ties("sensei310", &ties);
                    for p in &mut store.list {
                        p.apps.clear();
                    }
                    store.save();
                }
            }
        }
    }

    /// The devices a Library page lists: each one met, by its key, its name
    /// and its profiles (the keyboard's by their numbers, named as the
    /// keyboard names them; the mouse's as Ectomatter keeps them).
    fn lib_devices(&self) -> Vec<crate::games_page::Device> {
        let mut out = Vec::new();
        for (place, name, kind, _) in self.devices_in_order() {
            match place.as_str() {
                "z10" => {
                    let known = self.page.dev.is_some();
                    let profiles = z10::PROFILE_NUMBERS.iter().map(|n| (n.to_string(), if known { self.page.profile_name(*n) } else { format!("Profile {n}") })).collect();
                    out.push(crate::games_page::Device { key: "z10".into(), name, kind, profiles, glyph: None, sub: String::new(), chips: Vec::new() });
                }
                "mouse" => {
                    if let Some(s) = &self.mouse.profiles {
                        out.push(crate::games_page::Device { key: "sensei310".into(), name, kind, profiles: s.list.iter().map(|p| (p.id.clone(), p.name.clone())).collect(), glyph: None, sub: String::new(), chips: Vec::new() });
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The devices the packages Ectomatter runs itself brought in (the OpenRGB
    /// plugin's), as a Library page lists them: each by its own id, its
    /// profiles its package's, with its package's glyph.
    fn hosted_devices(&self) -> Vec<crate::games_page::Device> {
        self.hosted
            .devices()
            .into_iter()
            .map(|(core, d)| {
                let profiles = d["capabilities"]["profiles"]["list"].as_array().into_iter().flatten().filter_map(|p| Some((p["id"].as_str()?.to_string(), p["name"].as_str().unwrap_or("").to_string()))).collect();
                let glyph = self.packages.iter().position(|p| p.id() == core);
                let sub = match d["sub"].as_str().unwrap_or("") {
                    "" => format!("Through {}", self.hosted.get(&core).map_or(core.clone(), |p| p.package.name().to_string())),
                    s => s.to_string(),
                };
                crate::games_page::Device { key: d["device"].as_str().unwrap_or("").to_string(), name: d["name"].as_str().unwrap_or("").to_string(), kind: d["kind"].as_str().unwrap_or("").to_string(), profiles, glyph, sub, chips: Vec::new() }
            })
            .collect()
    }

    /// The Library's page as it is now, with each plugin's section for the page open.
    fn show_library(&mut self) {
        // (offscreen, what's known read at once: a snapshot has no later turn to take it on)
        if crate::desk::offscreen() && self.known.is_none() && self.hosted.launcher().is_some() {
            self.ask_known();
        }
        // the scene first, a row like a device's: No scene and every scene, each with its chip
        let chips = self.scene_chips();
        self.ui.set_scene_chips(ModelRc::new(VecModel::from(chips.clone())));
        let profiles = std::iter::once(("none".to_string(), "No scene".to_string())).chain(self.scenes().into_iter().map(|s| (s.id, s.name))).collect();
        let mut devices = vec![crate::games_page::Device { key: "scene".into(), name: "Scene".into(), kind: "scene".into(), profiles, glyph: None, sub: String::new(), chips }];
        devices.extend(self.lib_devices());
        devices.extend(self.hosted_devices());
        let sections = match self.games.open.clone() {
            Some(id) => {
                let kind = if id == crate::games::EVERY { "every" } else if self.games.library.is_game(&id) { "game" } else { "app" };
                let name = self.games.name(&id, &self.front_apps);
                // each plugin given the page's values; the one that says how games start, what's
                // known too (its section waits for that, the moment it takes to read)
                let launcher = self.hosted.launcher().map(String::from);
                let facts = self.launch_facts(&id);
                let lib = &self.games.library;
                let more = |p: &crate::packages::Package| {
                    let mut m = serde_json::json!({ "values": lib.wanted_values(&id, p.id()), "own": lib.plugin_values(&id, p.id()), "every": lib.plugin_values(crate::games::EVERY, p.id()) });
                    if launcher.as_deref() == Some(p.id()) {
                        m["facts"] = facts.clone();
                    }
                    m
                };
                let described = self.hosted.sections(kind, &name, &more);
                described
                    .into_iter()
                    .filter(|(plugin, _)| launcher.as_deref() != Some(plugin.as_str()) || !facts.is_null())
                    .map(|(plugin, desc)| crate::games_page::Section { name: self.hosted.get(&plugin).map(|p| p.package.name().to_string()).unwrap_or_default(), plugin, desc })
                    .collect()
            }
            None => Vec::new(),
        };
        self.games.show(&self.ui, &devices, sections, self.scale, &self.front_apps);
    }

    /// One of the Library's menus opened: the store's, the order's, or a
    /// device's on a page, under what opened it.
    fn open_games_menu(&mut self, m: crate::games_page::Menu, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let counts = self.games.counts(&self.front_apps);
        let rows = self.games.menu_rows(&m, counts);
        self.hang_menu(rows, under, anchor);
        self.games.menu = Some(m);
    }

    /// The packages, read again, and each one's glyph given to the window, at
    /// a device's row's size in the Library and a tile's in Settings.
    fn give_packages(&mut self) {
        self.packages = crate::packages::all();
        let at = |px: u32| -> Vec<slint::Image> { self.packages.iter().map(|p| p.glyph().map(|g| crate::package_glyph(&g, px)).unwrap_or_default()).collect() };
        let (small, tile) = (at(22), at(25));
        self.ui.set_package_glyphs(ModelRc::new(VecModel::from(small)));
        self.ui.set_package_tiles(ModelRc::new(VecModel::from(tile)));
    }

    /// Settings' Cores or Plugins: each package of that kind's row, by its place among all.
    fn package_rows(&self, kind: crate::packages::Kind) -> Vec<crate::ecto_settings::PackageRow> {
        self.packages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind() == kind)
            .map(|(i, p)| {
                let on = if p.id() == crate::z10::CORE { self.controlled } else { self.settings.on(&p.switch()) };
                let running = self.hosted.get(p.id());
                let status = if p.repository().is_none() { if p.added { "Added from a file".to_string() } else { "Comes with Ectomatter".to_string() } } else { crate::updates::status_of(p) };
                crate::ecto_settings::PackageRow {
                    name: p.name().to_string(),
                    description: p.description(),
                    can: p.can(),
                    version: p.version().to_string(),
                    status,
                    on,
                    warning: running.and_then(|r| r.warning.clone()).filter(|_| on),
                    glyph: i,
                    removable: p.added,
                    systems: p.systems().iter().map(|s| s.to_string()).collect(),
                }
            })
            .collect()
    }

    /// Settings' Devices: each core with its device as the system shows it
    /// (looked at every two seconds at most), what it's called in words about
    /// it, and what another program having it open means for it.
    fn device_rows(&mut self) -> Vec<crate::ecto_settings::DeviceRow> {
        let ids: Vec<(String, Option<(u16, u16)>)> = self.packages.iter().filter(|p| p.kind() == crate::packages::Kind::Core).map(|p| (p.id().to_string(), p.manifest.usb_ids())).collect();
        let rows = self.package_rows(crate::packages::Kind::Core);
        rows.into_iter()
            .zip(ids)
            .map(|(core, (id, usb))| {
                let st = match usb {
                    Some((vendor, product)) => {
                        if self.device_status.get(&id).is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(2)) {
                            self.device_status.insert(id.clone(), (Instant::now(), crate::devices::usb(vendor, product)));
                        }
                        self.device_status[&id].1.clone()
                    }
                    None => serde_json::Value::Null,
                };
                let kind = self.device_kind(&id);
                let holders_note = if kind == "keyboard" {
                    "Two programs sending lighting at once make the keys flicker. Close the other program, or turn Live lighting off here.".to_string()
                } else {
                    format!("Programs that both set the {kind} undo each other’s changes: whichever sets its lights or buttons last wins. Ectomatter changes the {kind} only when you do, here.")
                };
                crate::ecto_settings::DeviceRow {
                    core,
                    present: st["present"].as_bool().unwrap_or(false),
                    reachable: st["reachable"].as_bool(),
                    holders: st["holders"].as_array().map(|a| a.iter().filter_map(|h| h["name"].as_str().map(String::from)).collect()).unwrap_or_default(),
                    holders_note,
                    kind,
                }
            })
            .collect()
    }

    /// What a core's device is called in words about it ("keyboard"), as its
    /// own description says; the keyboard's and the mouse's before they're met.
    fn device_kind(&self, id: &str) -> String {
        let told = if id == crate::z10::CORE {
            crate::cores::shown().device["kind"].as_str().map(String::from).or(Some("keyboard".into()))
        } else if id == crate::mouse::CORE {
            self.mouse.state["device"]["kind"].as_str().map(String::from).or(Some("mouse".into()))
        } else {
            self.hosted.devices().into_iter().find(|(p, _)| p == id).and_then(|(_, d)| d["kind"].as_str().map(String::from))
        };
        told.unwrap_or_else(|| "device".into())
    }

    /// Each running plugin with a Settings section of its own handed all the
    /// values Ectomatter keeps for it (`only` the one named, where one is), as
    /// it starts and as one changes; handed, not waited on.
    fn hand_values(&self, only: Option<&str>) {
        for r in &self.hosted.plugins {
            let id = r.package.id();
            let has = r.package.manifest.all["adds"].as_array().is_some_and(|a| a.iter().any(|x| x == "settings"));
            if !has || only.is_some_and(|o| o != id) {
                continue;
            }
            let values = self.settings.get(&format!("plugins.{id}")).filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
            r.core.tell("settings/set", &serde_json::json!({ "values": values }));
        }
    }

    /// A package started, and if it's a plugin with a Settings section of its
    /// own, handed its values.
    fn start_package(&mut self, p: crate::packages::Package) {
        let id = p.id().to_string();
        self.hosted.start(p);
        self.hand_values(Some(&id));
    }

    /// One of a plugin's own settings set, kept, and handed to the plugin.
    fn set_plugin_value(&mut self, plugin: &str, setting: &str, v: serde_json::Value) {
        if let Err(e) = self.settings.set(&format!("plugins.{plugin}.{setting}"), v) {
            self.status(format!("The setting was not saved: {e}"), 2);
            return;
        }
        self.hand_values(Some(plugin));
        self.refresh();
    }

    /// The plugins' own Settings sections, each with the values Ectomatter
    /// keeps for it and what's being typed into one of its boxes.
    fn plugin_sections(&self) -> Vec<crate::ecto_settings::PluginSection> {
        self.hosted
            .settings_sections()
            .into_iter()
            .map(|(plugin, section)| {
                let values = self.settings.get(&format!("plugins.{plugin}")).unwrap_or(serde_json::Value::Null);
                let typed = self.typing.as_ref().filter(|(p, _, _)| *p == plugin).map(|(_, s, t)| (s.clone(), t.clone()));
                crate::ecto_settings::PluginSection { plugin, section, values, typed }
            })
            .collect()
    }

    /// The weather outside, as the Weather Feed last told it: in °C, and
    /// the sky in words.
    fn outside(&self) -> Option<(f64, String)> {
        let r = self.hosted.readings();
        Some((r.value("weather", "temperature")?.as_f64()?, r.value("weather", "sky")?.as_str()?.to_string()))
    }

    /// Add a core or Add a plugin, from Settings: its form.
    fn add_package_form(&mut self, kind: crate::packages::Kind) {
        self.package_form = Some(PackageForm { kind, repo: None, file: None, found: None, why: None, typed_at: None, looked_for: None, looking: None, picking: None, adding: None });
        self.ui.set_gl_plugin_address("".into());
        self.show_package_form();
    }

    /// The form as it stands: what was found, and whether it can be added.
    fn show_package_form(&mut self) {
        let Some(f) = &self.package_form else { return };
        self.ui.set_gl_modal(4);
        let word = f.kind.word();
        self.ui.set_gl_plugin_title(format!("Add a {word}").into());
        self.ui.set_gl_plugin_sub(format!("From the {word}’s GitHub releases, or from its file. Ectomatter says what the {word} can do before you add it.").into());
        // (the file by its name: the folder it's in says less than its name)
        self.ui.set_gl_plugin_file(f.file.as_ref().map(|p| p.file_name().map_or_else(|| p.to_string_lossy().to_string(), |n| n.to_string_lossy().to_string())).unwrap_or_default().into());
        let (mut can_add, mut state, mut bad) = (false, String::new(), false);
        match &f.found {
            Some(m) => {
                let name = m["name"].as_str().unwrap_or("This package");
                let version = m["version"].as_str().unwrap_or("");
                // (a core found from Add a plugin, or the other way round, goes where its kind goes)
                let its = crate::packages::Kind::of(m);
                self.ui.set_gl_plugin_found(format!("{name} {version}").trim().into());
                self.ui.set_gl_plugin_found_sub(m["description"].as_str().unwrap_or("").into());
                self.ui.set_gl_plugin_can(crate::packages::can_words(m).join("\n").into());
                let id = m["id"].as_str().unwrap_or("");
                let have = self.packages.iter().find(|p| p.id() == id);
                if let Err(why) = crate::updates::runs_here(m) {
                    (state, bad) = (format!("This Ectomatter can’t use it: {why}."), true);
                } else if let Some(h) = have.filter(|h| crate::packages::version_key(h.version()) >= crate::packages::version_key(version)) {
                    state = format!("Ectomatter already has {} {}.", h.name(), h.version());
                } else if f.adding.is_some() {
                    state = "Adding…".into();
                } else {
                    can_add = true;
                    if its != f.kind {
                        state = match its {
                            crate::packages::Kind::Core => format!("{name} is a core, for a device, so it goes under Devices."),
                            crate::packages::Kind::Plugin => format!("{name} is a plugin, so it goes under Plugins."),
                        };
                    }
                }
            }
            None => {
                self.ui.set_gl_plugin_found("".into());
                self.ui.set_gl_plugin_found_sub("".into());
                self.ui.set_gl_plugin_can("".into());
                if let Some(why) = &f.why {
                    (state, bad) = (why.clone(), true);
                } else if f.looking.is_some() {
                    state = format!("Looking for {} on GitHub…", f.looked_for.clone().unwrap_or_default());
                }
            }
        }
        self.ui.set_gl_plugin_state(state.into());
        self.ui.set_gl_plugin_bad(bad);
        self.ui.set_gl_can_add(can_add);
    }

    /// What came of the form's looking: an address looked up once its typing
    /// rests, a file read once it's chosen, a plugin put in place.
    fn package_form_tick(&mut self) {
        let typed = self.ui.get_gl_plugin_address().to_string();
        let Some(f) = &mut self.package_form else { return };
        let mut changed = false;
        if f.typed_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(500)) {
            f.typed_at = None;
            let repo = crate::packages::repository_of(&typed);
            if repo != f.looked_for {
                f.found = None;
                f.why = None;
                f.looked_for = repo.clone();
                f.repo = repo.clone();
                f.looking = repo.map(|r| {
                    let (tx, rx) = mpsc::channel();
                    let _ = std::thread::Builder::new().name("plugin look".into()).spawn(move || {
                        let _ = tx.send(crate::updates::release_manifest(&r));
                    });
                    rx
                });
                if f.looking.is_none() && !typed.trim().is_empty() {
                    f.why = Some("That isn’t a GitHub address. It looks like github.com/owner/name.".into());
                }
                changed = true;
            }
        }
        if let Some(rx) = &f.looking {
            if let Ok(r) = rx.try_recv() {
                f.looking = None;
                match r {
                    Ok(m) => f.found = Some(m),
                    Err(e) => f.why = Some(format!("No plugin there: {e}.")),
                }
                changed = true;
            }
        }
        if let Some(rx) = &f.picking {
            if let Ok(chosen) = rx.try_recv() {
                f.picking = None;
                if let Some(p) = chosen {
                    f.repo = None;
                    f.looked_for = None;
                    match crate::packages::manifest_in_file(&p) {
                        Ok(m) => {
                            f.found = Some(m);
                            f.why = None;
                        }
                        Err(e) => {
                            f.found = None;
                            f.why = Some(e);
                        }
                    }
                    f.file = Some(p);
                    self.ui.set_gl_plugin_address("".into());
                }
                changed = true;
            }
        }
        if let Some(rx) = &f.adding {
            if let Ok(r) = rx.try_recv() {
                f.adding = None;
                match r {
                    Ok(m) => {
                        self.package_form = None;
                        self.ui.set_gl_modal(0);
                        self.package_added(&m.id, &m.name);
                        return;
                    }
                    Err(e) => f.why = Some(format!("It couldn’t be added: {e}.")),
                }
                changed = true;
            }
        }
        if changed {
            self.show_package_form();
        }
    }

    /// Add, in Add a plugin: the plugin put in place (from GitHub on a thread
    /// of its own; from a file at once), then started.
    fn package_add_done(&mut self) {
        let Some(f) = &mut self.package_form else { return };
        let Some(m) = f.found.clone() else { return };
        let id = m["id"].as_str().unwrap_or("").to_string();
        if let Some(repo) = f.repo.clone() {
            let (tx, rx) = mpsc::channel();
            let _ = std::thread::Builder::new().name("plugin add".into()).spawn(move || {
                let _ = tx.send(crate::updates::install_latest(&repo, &id, &crate::packages::added_dir(crate::packages::Kind::of(&m))));
            });
            f.adding = Some(rx);
            self.show_package_form();
            return;
        }
        if let Some(file) = f.file.clone() {
            match crate::packages::install(&file) {
                Ok(m) => {
                    self.package_form = None;
                    self.ui.set_gl_modal(0);
                    self.package_added(&m.id, &m.name);
                }
                Err(e) => {
                    f.why = Some(format!("It couldn’t be added: {e}"));
                    self.show_package_form();
                }
            }
        }
    }

    /// A plugin added: in Settings' list, and started.
    fn package_added(&mut self, id: &str, name: &str) {
        self.give_packages();
        if let Some(p) = self.packages.iter().find(|p| p.id() == id).cloned() {
            if !crate::hosted::started_elsewhere(id) && self.settings.on(&p.switch()) {
                self.hosted.stop(id);
                self.start_package(p);
            }
        }
        self.status(format!("Added {name}."), 1);
        self.refresh();
    }

    /// An added plugin taken out: what it changed put back, stopped, its folder
    /// gone; one that came with Ectomatter too is used again in its place.
    fn remove_package(&mut self, id: &str) {
        let name = self.packages.iter().find(|p| p.id() == id).map(|p| p.name().to_string()).unwrap_or_else(|| id.to_string());
        self.worker.drop_plugin(id);
        self.hosted.stop(id);
        self.following.retain(|k, _| !k.starts_with(&format!("{id}/")));
        self.plugin_applied.retain(|k, _| !k.starts_with(&format!("{id}/")));
        let kind = self.packages.iter().find(|p| p.id() == id).map_or(crate::packages::Kind::Plugin, |p| p.kind());
        let root = crate::packages::added_dir(kind);
        let dir = root.join(id);
        if dir.starts_with(&root) && dir != root {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                self.status(format!("{name} wasn’t removed: {e}"), 2);
                return;
            }
        }
        self.give_packages();
        if let Some(p) = self.packages.iter().find(|p| p.id() == id).cloned() {
            if self.settings.on(&p.switch()) && !crate::hosted::started_elsewhere(id) {
                self.start_package(p);
            }
        }
        self.hosted.forget_sections();
        self.status(format!("Removed {name}."), 1);
        self.refresh();
    }

    /// A package turned on or off in Settings: the keyboard's core is the
    /// keyboard's Control, and it and the mouse's core start again with
    /// Ectomatter; a plugin starts or stops at once, what it changed for a game
    /// running put back first.
    fn package_switch(&mut self, row: usize, on: bool) {
        let Some(p) = self.packages.get(row).cloned() else { return };
        if p.id() == crate::z10::CORE {
            self.cell_pick(Ctl::Control, if on { 0 } else { 1 }, (0.0, 0.0, 0.0), 0.0);
            return;
        }
        if self.settings.on(&p.switch()) == on {
            return;
        }
        if let Err(e) = self.settings.set(&p.switch(), serde_json::json!(on)) {
            self.status(format!("The setting was not saved: {e}"), 2);
            return;
        }
        if p.id() == crate::mouse::CORE {
            self.restart = Some("settings".into());
            return;
        }
        if on {
            self.start_package(p);
        } else {
            self.worker.drop_plugin(p.id());
            self.hosted.stop(p.id());
            // (its devices let go of what they followed)
            self.following.retain(|k, _| !k.starts_with(&format!("{}/", p.id())));
            self.plugin_applied.retain(|k, _| !k.starts_with(&format!("{}/", p.id())));
        }
        self.hosted.forget_sections();
    }

    /// An app that doesn't run through Steam, as its windows open and close:
    /// its plugins' changes made as its first window opens (where its page
    /// has any), and put back as its last closes. Games, and Steam's apps,
    /// go by the Library's watch instead.
    fn app_windows(&mut self, f: &crate::front_app::Front, app: &str) {
        if f.window.is_empty() || f.is_ectomatter() {
            return;
        }
        if f.closed {
            if self.windows.remove(&f.window).is_some() && !self.windows.values().any(|a| a == app) && self.app_sessions.remove(app) {
                eprintln!("ectomatter: {} closed", self.games.name(app, &self.front_apps));
                self.worker.quit_game(app);
            }
            return;
        }
        self.windows.insert(f.window.clone(), app.to_string());
        let watched = self.games.running.iter().any(|r| r.id == app) || crate::games::steam_number(app).is_some() || self.games.library.is_game(app);
        if watched || self.app_sessions.contains(app) {
            return;
        }
        let name = self.games.name(app, &self.front_apps);
        let r = crate::games::Running { id: app.to_string(), pid: f.pid, since: crate::games::now() };
        let plan = self.game_plan(&r, &name);
        if !plan.changes.is_empty() {
            eprintln!("ectomatter: {name} opened");
            self.app_sessions.insert(app.to_string());
            self.worker.start_game(plan);
        }
    }

    /// What the plugins change as a game or app starts, for the put-back
    /// thread to make: each plugin's changes for it from its choices (its own
    /// over Every game's) and the person's commands for it.
    fn game_plan(&self, r: &crate::games::Running, name: &str) -> crate::putback::Plan {
        let lib = &self.games.library;
        let kind = if lib.is_game(&r.id) { "game" } else { "app" };
        let scripts = lib.scripts_for(&r.id);
        let game = crate::putback::Game { id: r.id.clone(), name: name.to_string(), pid: r.pid, since: r.since };
        let values = |plugin: &str| lib.wanted_values(&r.id, plugin);
        let mut changes: Vec<(String, serde_json::Value, String, String, Option<crate::commands::Script>)> = self.hosted.changes(&game, kind, &values).into_iter().map(|(plugin, what, label, words)| (plugin, what, label, words, None)).collect();
        // then the person's own commands, Ectomatter's Custom Scripts: Every game's first, so
        // their undos come last as the game quits
        for (s, _) in scripts {
            let label: String = if s.name.trim().is_empty() { "SCRIPT".to_string() } else { s.name.to_uppercase() }.chars().take(20).collect();
            changes.push((CUSTOM_SCRIPTS.to_string(), serde_json::json!({ "command": s.id }), label, s.name.clone(), Some(s)));
        }
        crate::putback::Plan { game, changes, seq: PLANS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) }
    }

    // ------------------------------------------------------------ games started through Ectomatter

    /// What the plugin that says how games start (the Game Configurator) is
    /// told of a page: what's known of this computer, and for a Steam game its
    /// own (its Steam number, whether it runs without Proton, whether it
    /// starts through Ectomatter, its prefix and its backups), for a game
    /// added by hand that it's one; for Every game's page, how many games
    /// start through Ectomatter. Null until what's known is read.
    fn launch_facts(&self, id: &str) -> serde_json::Value {
        let Some(k) = &self.known else { return serde_json::Value::Null };
        let mut f = k.facts();
        let games: Vec<&crate::steam::App> = self.games.library.steam.as_ref().map(|s| s.apps.iter().filter(|a| a.kind == crate::steam::Kind::Game).collect()).unwrap_or_default();
        if id == crate::games::EVERY {
            f["through"] = serde_json::json!([games.iter().filter(|a| k.through(a.id)).count(), games.len()]);
        } else if let Some(app) = crate::games::steam_number(id) {
            let a = games.iter().find(|a| a.id == app);
            let lib = a.map_or_else(|| k.root.clone(), |a| a.library.clone());
            let (bytes, made, backups) = k.prefixes.get(&app).copied().unwrap_or((0, false, 0));
            f["game"] = serde_json::json!({
                "id": id, "name": self.games.name(id, &self.front_apps), "app": app, "native": a.is_some_and(|a| k.native(app, a.linux)), "through": k.through(app),
                "prefix": { "path": crate::launcher::prefix_of(&lib, app), "bytes": bytes, "made": made }, "backups": backups,
            });
            // (until it starts through Ectomatter, the build Steam starts it with; a Linux
            // version, the runtime Steam starts it in)
            if !k.through(app) {
                f["game"]["steam"] = serde_json::json!(k.steam_build(app));
            }
            if a.is_some_and(|a| k.native(app, a.linux)) {
                f["game"]["runtime"] = serde_json::json!(k.runtime_of(app, a.and_then(|a| a.runtime.as_deref())));
            }
        } else {
            f["game"] = serde_json::json!({ "id": id, "name": self.games.name(id, &self.front_apps), "hand": true });
        }
        f
    }

    /// A game's recipe for the launcher, as the plugin that says how games
    /// start answers for it (Every game's, for games without one of their own).
    fn recipe_of(&self, id: &str) -> Option<serde_json::Value> {
        let facts = self.launch_facts(id);
        if facts.is_null() {
            return None;
        }
        let every = id == crate::games::EVERY;
        let name = if every { String::new() } else { self.games.name(id, &self.front_apps) };
        let lib = &self.games.library;
        let values = |plugin: &str| lib.wanted_values(id, plugin);
        let every_values = |plugin: &str| lib.plugin_values(crate::games::EVERY, plugin);
        let game = serde_json::json!({ "id": id, "name": name, "app": crate::games::steam_number(id).unwrap_or(0) });
        match self.hosted.launch(&game, &values, &every_values, &facts)? {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("ectomatter: how {} starts: {e}", if every { "every game" } else { name.as_str() });
                None
            }
        }
    }

    /// Each Steam game's recipe written for the launcher, and Every game's
    /// (launch/default.json), when the plugin's choices or what's known have
    /// changed since they were last written. None offscreen: a snapshot
    /// writes nothing.
    fn write_recipes(&mut self) {
        if crate::desk::offscreen() {
            return;
        }
        let (Some(plugin), Some(k)) = (self.hosted.launcher().map(String::from), self.known.clone()) else { return };
        let stamp = (self.games.library.plugin_all(&plugin), k.clone());
        if self.recipes_for.as_ref() == Some(&stamp) {
            return;
        }
        let ids: Vec<String> = self.games.library.steam.as_ref().map(|s| s.apps.iter().filter(|a| a.kind == crate::steam::Kind::Game).map(|a| crate::games::steam_id(a.id)).collect()).unwrap_or_default();
        for id in std::iter::once(crate::games::EVERY.to_string()).chain(ids) {
            let Some(r) = self.recipe_of(&id) else { continue };
            let file = crate::games::steam_number(&id).map_or_else(|| "default".to_string(), |n| n.to_string());
            if let Err(e) = crate::launcher::write_recipe(&file, &r) {
                eprintln!("ectomatter: the recipe {file}.json wasn’t written: {e}");
            }
        }
        self.recipes_for = Some(stamp);
    }

    /// What's known read again (on its thread; offscreen, now), with the
    /// prefix of the game whose page is open.
    fn ask_known(&mut self) {
        let Some(root) = self.games.library.steam.as_ref().map(|s| s.root.clone()) else { return };
        self.known_asked = Some(Instant::now());
        let prefixes: Vec<(u32, std::path::PathBuf)> = self
            .games
            .open
            .as_deref()
            .and_then(crate::games::steam_number)
            .and_then(|app| self.games.library.steam.as_ref()?.apps.iter().find(|a| a.id == app).map(|a| (app, a.library.clone())))
            .into_iter()
            .collect();
        if crate::desk::offscreen() {
            self.known = Some(crate::launcher::know(&root, &prefixes));
            return;
        }
        self.facts_reader.get_or_insert_with(crate::launcher::Reader::start).ask(root, prefixes);
    }

    /// A game starting through Ectomatter's launcher, which waits: its
    /// changes made now, before the Library's watch would see the game, by
    /// Steam's reaper for it; the launcher answered once they're made.
    pub fn game_starting(&mut self, s: crate::launcher::Start) {
        // (a shortcut to a program outside Steam has no Steam number to know it by:
        // the Library's watch makes its changes, by its program)
        if s.id == crate::games::steam_id(0) {
            let _ = s.done.send(());
            return;
        }
        match self.starting.get(&s.id).copied() {
            // (seen by the watch first: its changes are made, or being made)
            Some((_, true)) => {
                let id = s.id.clone();
                self.let_start(&id, s.done);
                return;
            }
            Some((seq, false)) => {
                self.holds.push((s.id, seq, s.done, Instant::now()));
                return;
            }
            None => {}
        }
        let reaper = crate::launcher::reaper_above(s.pid).unwrap_or(s.pid);
        // (by the Library's name for it; a game it doesn't hold yet, by its recipe's)
        let name = match self.games.name(&s.id, &self.front_apps) {
            n if n == s.id && !s.name.is_empty() => s.name.clone(),
            n => n,
        };
        eprintln!("ectomatter: {name} starting through Ectomatter");
        let r = crate::games::Running { id: s.id.clone(), pid: reaper, since: crate::games::now() };
        let plan = self.game_plan(&r, &name);
        let seq = plan.seq;
        self.worker.start_game(plan);
        self.starting.insert(s.id.clone(), (seq, false));
        self.early.push((s.id.clone(), reaper, Instant::now()));
        self.holds.push((s.id, seq, s.done, Instant::now()));
    }

    /// A launcher waiting on a game answered: the game's recipe written
    /// afresh first, with the main monitor as the game's changes left it (in
    /// HDR, say, for gamescope to show the game in HDR).
    fn let_start(&mut self, id: &str, done: mpsc::Sender<()>) {
        if !crate::desk::offscreen() {
            if let Some(k) = &mut self.known {
                k.main = crate::launcher::main_monitor();
            }
            if let (Some(r), Some(n)) = (self.recipe_of(id), crate::games::steam_number(id)) {
                if let Err(e) = crate::launcher::write_recipe(&n.to_string(), &r) {
                    eprintln!("ectomatter: the recipe {n}.json wasn’t written: {e}");
                }
            }
        }
        let _ = done.send(());
    }

    /// A game's start changes made, as the put-back thread says: the
    /// launchers waiting on it answered.
    fn started_through(&mut self, id: &str, seq: u64) {
        // (an older plan's word for the same game answers nothing of a newer start)
        match self.starting.get_mut(id) {
            Some(s) if s.0 == seq => s.1 = true,
            _ => return,
        }
        let (waiting, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.holds).into_iter().partition(|(g, n, _, _)| g == id && *n == seq);
        self.holds = rest;
        for (_, _, done, _) in waiting {
            self.let_start(id, done);
        }
    }

    /// Each turn: what's known read again when it's due (each minute; each
    /// few seconds while a page of the Library shows), the launcher installed
    /// in Steam once, the recipes kept up to date, the waits past their time
    /// let go, a start made early whose game never showed put back, a program
    /// chosen to run in a prefix started, and the jobs finished said. True if
    /// the Library has news.
    fn launch_tick(&mut self) -> bool {
        // (the launchers waiting, and starts made early, looked after first: the tool stays
        // in Steam's settings while the plugin that says how games start is off)
        self.holds.retain(|(_, _, _, t)| t.elapsed() < Duration::from_secs(31));
        let gone: Vec<String> = self.early.iter().filter(|(_, pid, t)| t.elapsed() > Duration::from_secs(10) && !std::path::Path::new(&format!("/proc/{pid}")).exists()).map(|(id, _, _)| id.clone()).collect();
        for id in gone {
            self.early.retain(|(g, _, _)| *g != id);
            if !self.games.running.iter().any(|r| r.id == id) {
                eprintln!("ectomatter: {} ended before it was seen running", self.games.name(&id, &self.front_apps));
                self.worker.quit_game(&id);
                self.starting.remove(&id);
            }
        }
        if self.hosted.launcher().is_none() {
            return false;
        }
        let mut news = false;
        let showing = self.ui.get_place() == "library" && self.games.open.is_some();
        if self.known_asked.is_none_or(|t| t.elapsed() > Duration::from_secs(if showing { 5 } else { 60 })) {
            self.ask_known();
        }
        if let Some(r) = &self.facts_reader {
            while let Ok(k) = r.known.try_recv() {
                if self.known.as_ref() != Some(&k) {
                    self.known = Some(k);
                    news = true;
                }
            }
        }
        if !self.tool_installed && !crate::desk::offscreen() {
            if let Some(root) = self.games.library.steam.as_ref().map(|s| s.root.clone()) {
                self.tool_installed = true;
                let _ = std::thread::Builder::new().name("launcher-install".into()).spawn(move || match crate::launcher::install(&root) {
                    Ok(true) => eprintln!("ectomatter: the launcher is installed in Steam (Steam lists it under Compatibility once it starts again)"),
                    Ok(false) => {}
                    Err(e) => eprintln!("ectomatter: the launcher wasn’t installed in Steam: {e}"),
                });
            }
        }
        self.write_recipes();
        if let Some((id, rx)) = &self.run_pick {
            if let Ok(chosen) = rx.try_recv() {
                let id = id.clone();
                self.run_pick = None;
                if let Some(p) = chosen {
                    self.in_prefix(&id, vec![p.to_string_lossy().into_owned()]);
                }
            }
        }
        let mut said = Vec::new();
        self.jobs.retain(|rx| match rx.try_recv() {
            Ok(s) => {
                said.push(s);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        for (words, bad) in said {
            self.games.say(words, bad);
            self.ask_known();
            news = true;
        }
        news
    }

    /// The Proton build a game starts with (its own, else Every game's, else
    /// Steam's default), as installed.
    fn build_for(&self, id: &str) -> Option<crate::launcher::Build> {
        let k = self.known.as_ref()?;
        let plugin = self.hosted.launcher()?;
        let want = self.games.library.wanted_values(id, plugin)["version"].as_str().map(String::from).unwrap_or_else(|| k.default.clone());
        k.builds.iter().find(|b| b.id == want).or_else(|| k.builds.iter().find(|b| b.id == k.default)).or_else(|| k.builds.first()).cloned()
    }

    /// A Steam game's number and library, for its prefix.
    fn prefix_place(&self, id: &str) -> Option<(u32, std::path::PathBuf)> {
        let app = crate::games::steam_number(id)?;
        let lib = self.games.library.steam.as_ref()?.apps.iter().find(|a| a.id == app)?.library.clone();
        Some((app, lib))
    }

    /// A program run in a game's prefix with the game's Proton build (Wine's
    /// configuration, Winetricks, or one the person chose).
    fn in_prefix(&mut self, id: &str, what: Vec<String>) {
        let name = self.games.name(id, &self.front_apps);
        let (Some((app, lib)), Some(b), Some(root)) = (self.prefix_place(id), self.build_for(id), self.known.as_ref().map(|k| k.root.clone())) else {
            self.games.say(format!("Ectomatter can’t find {name}’s prefix or a Proton build for it."), true);
            return;
        };
        if what.first().is_some_and(|w| w == "winetricks") && !crate::launcher::on_path("winetricks") {
            self.games.say("Winetricks isn’t installed. Install it from your system’s software center, then try again.", true);
            return;
        }
        match crate::launcher::in_prefix(&root, &lib, app, &b.path, &what).spawn() {
            Ok(_) => self.games.say(format!("Opening it in {name}’s prefix, with {}.", b.name), false),
            Err(e) => self.games.say(format!("It didn’t open: {e}"), true),
        }
    }

    /// One of the Game Configurator's actions on the open page.
    fn launch_action(&mut self, action: &str) {
        let Some(id) = self.games.open.clone() else { return };
        let name = self.games.name(&id, &self.front_apps);
        let running = self.games.running.iter().any(|r| r.id == id);
        match action {
            "through" => {
                if let Some(app) = crate::games::steam_number(&id) {
                    self.start_through(vec![app]);
                }
            }
            "through-all" => {
                let Some(k) = &self.known else { return };
                let apps: Vec<u32> = self.games.library.steam.as_ref().map(|s| s.apps.iter().filter(|a| a.kind == crate::steam::Kind::Game && !k.through(a.id)).map(|a| a.id).collect()).unwrap_or_default();
                self.start_through(apps);
            }
            "open-prefix" => {
                if let Some((app, lib)) = self.prefix_place(&id) {
                    let _ = std::process::Command::new("xdg-open").arg(crate::launcher::prefix_of(&lib, app)).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
                }
            }
            "wine-configuration" => self.in_prefix(&id, vec!["winecfg".into()]),
            "winetricks" => self.in_prefix(&id, vec!["winetricks".into()]),
            "run-in-prefix" => {
                if self.run_pick.is_none() {
                    self.run_pick = Some((id.clone(), crate::picker::open_file(&format!("Choose a program to run in {name}’s prefix"), "Windows programs", &["*.exe", "*.msi", "*.bat"])));
                }
            }
            "back-up-prefix" | "reset-prefix" => {
                if running {
                    self.games.say(format!("Quit {name} first: its prefix is in use."), true);
                    return;
                }
                let Some((app, lib)) = self.prefix_place(&id) else { return };
                let reset = action == "reset-prefix";
                let (tx, rx) = mpsc::channel();
                self.games.say(if reset { format!("Backing up {name}’s prefix, then resetting it…") } else { format!("Backing up {name}’s prefix…") }, false);
                let _ = std::thread::Builder::new().name("prefix".into()).spawn(move || {
                    let r = if reset { crate::launcher::reset(&lib, app) } else { crate::launcher::back_up(&lib, app) };
                    let _ = tx.send(match r {
                        Ok(p) if reset => (format!("Reset {name}’s prefix. Its backup is in {}.", crate::launcher::shown(&p)), false),
                        Ok(p) => (format!("Backed up {name}’s prefix to {}.", crate::launcher::shown(&p)), false),
                        Err(e) => (e, true),
                    });
                });
                self.jobs.push(rx);
            }
            _ => {}
        }
    }

    /// Games set to start through Ectomatter in Steam (each of `apps`, by
    /// its Steam number): each one's Proton build and game-helper line
    /// carried over into the Library first, then Steam closed, its settings
    /// changed (the line taken out), and Steam opened again if it was open.
    /// Refused while a Steam game runs, which Steam closing would end.
    fn start_through(&mut self, apps: Vec<u32>) {
        if apps.is_empty() || self.steam_busy.as_ref().is_some_and(|b| b.load(std::sync::atomic::Ordering::Relaxed)) {
            return;
        }
        if let Some(r) = self.games.running.iter().find(|r| crate::games::steam_number(&r.id).is_some()) {
            let n = self.games.name(&r.id, &self.front_apps);
            self.games.say(format!("Quit {n} first: Steam has to close for this change."), true);
            return;
        }
        let (Some(k), Some(plugin)) = (self.known.clone(), self.hosted.launcher().map(String::from)) else { return };
        let lines = crate::launcher::launch_options(&k.root);
        let mouse = self.mouse.profiles.as_ref().and_then(crate::launcher::game_profile);
        let linux: Vec<u32> = self.games.library.steam.as_ref().map(|s| s.apps.iter().filter(|a| k.native(a.id, a.linux)).map(|a| a.id).collect()).unwrap_or_default();
        let which: Vec<(u32, bool, bool)> = apps.iter().map(|a| (*a, linux.contains(a), true)).collect();
        let switches = crate::launcher::carry_over(&mut self.games.library, &k, &which, &lines, mouse.as_deref(), &plugin);
        let n = apps.len();
        let busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        self.steam_busy = Some(busy.clone());
        let (tx, rx) = mpsc::channel();
        let backups = crate::data_dir().join("steam-backups");
        let root = k.root.clone();
        let one = (n == 1).then(|| self.games.name(&crate::games::steam_id(apps[0]), &self.front_apps));
        self.games.say("Closing Steam to change its settings…", false);
        let _ = std::thread::Builder::new().name("steam-settings".into()).spawn(move || {
            // (looked at again as Steam is asked to close, which would end a game running)
            if !crate::steam::running().is_empty() {
                let _ = tx.send(("Quit the game first: Steam has to close for this change.".to_string(), true));
                busy.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            let was_open = crate::launcher::steam_running();
            // (the launcher in place first, for Steam to list it as it opens again)
            let r = crate::launcher::install(&root).and_then(|_| crate::launcher::close_steam()).and_then(|_| crate::launcher::change_steam(&root, &switches, &backups));
            let reopened = !was_open || crate::launcher::open_steam();
            let _ = tx.send(match r {
                Ok(_) => {
                    let done = match one {
                        Some(g) => format!("{g} starts through Ectomatter now."),
                        None => format!("{n} games start through Ectomatter now."),
                    };
                    if reopened { (done, false) } else { (format!("{done} Steam didn’t open again: open it from your apps."), true) }
                }
                Err(e) => (format!("Steam’s settings weren’t changed: {e}"), true),
            });
            busy.store(false, std::sync::atomic::Ordering::Relaxed);
        });
        self.jobs.push(rx);
    }

    /// What the put-back thread said: a command run (kept with it), what was
    /// put back after Ectomatter stopped short (said, here and as a notice), a
    /// change that couldn't be made. True if the Library has news.
    fn worker_told(&mut self) -> bool {
        let mut news = false;
        while let Ok(t) = self.worker.told.try_recv() {
            news = true;
            match t {
                crate::putback::Told::Ran { script, game, quit, ran, at } => self.games.script_ran(script, &game, quit, &ran, at),
                crate::putback::Told::PutBack { name, words, after_stop } => {
                    eprintln!("ectomatter: put back for {name}: {}", words.join(", "));
                    if after_stop && !words.is_empty() {
                        let said = format!("Ectomatter stopped while {name} ran, so it put back {} now.", crate::text::list(&words));
                        self.games.say(said.clone(), false);
                        crate::tray::notify("Ectomatter put things back", &said);
                    }
                }
                crate::putback::Told::Failed { words, why } => {
                    eprintln!("ectomatter: couldn’t change {words}: {why}");
                    self.games.say(format!("Ectomatter couldn’t change {words}: {why}"), true);
                }
                crate::putback::Told::Started { id, seq } => self.started_through(&id, seq),
            }
        }
        news
    }

    /// Games started and quit, as the Library's watch saw them: the window in
    /// front looked at again (a game's window may have come up before its
    /// game was seen), the plugins' changes made or put back, and the
    /// keyboard's screen told.
    fn games_seen(&mut self, events: Vec<crate::games_page::Event>) {
        use crate::lcd_player::{Control, GameSeen};
        for e in &events {
            let (crate::games_page::Event::Started(r) | crate::games_page::Event::Quit(r)) = e;
            let name = self.games.name(&r.id, &self.front_apps);
            let started = matches!(e, crate::games_page::Event::Started(_));
            eprintln!("ectomatter: {name} {}", if started { "started" } else { "quit" });
            // what the plugins change, for games and apps alike (SteamVR's monitor turned off)
            if started {
                if self.starting.contains_key(&r.id) {
                    // (started through Ectomatter: its changes were made as its launcher asked)
                    self.early.retain(|(g, _, _)| *g != r.id);
                } else {
                    let plan = self.game_plan(r, &name);
                    self.starting.insert(r.id.clone(), (plan.seq, false));
                    self.worker.start_game(plan);
                }
            } else {
                self.worker.quit_game(&r.id);
                self.starting.remove(&r.id);
                self.early.retain(|(g, _, _)| *g != r.id);
            }
            // (an app Steam starts, such as SteamVR or Wallpaper Engine, is no game starting)
            if !self.games.library.is_game(&r.id) {
                continue;
            }
            let entry = self.games.library.entries(&|id| self.front_apps.name(id)).into_iter().find(|x| x.id == r.id);
            let game = if started {
                crate::lcd_page::Game { name, since: r.since, ended: 0, minutes: entry.as_ref().map_or(0, |x| x.minutes), last: entry.as_ref().map_or(0, |x| x.last), changing: String::new(), done: (0, 0) }
            } else {
                let mut g = self.game_now.clone().filter(|g| g.name == name).unwrap_or_else(|| crate::lcd_page::Game { name, since: r.since, ..Default::default() });
                g.ended = crate::games::now();
                g
            };
            self.game_now = Some(game.clone());
            if let Some(pl) = &self.player {
                pl.tell(Control::Game(if started { GameSeen::Started(game) } else { GameSeen::Quit(game) }));
            }
        }
        if let Some(f) = self.last_front.clone() {
            self.front_app(f);
        }
    }

    /// An app come to the front, as KWin says (see front_app.rs): each device
    /// switches to its profile tied to the app; once an app it followed
    /// leaves the front, back to the profile chosen by hand. Ectomatter's own
    /// windows change nothing, so a profile can be changed with Ectomatter in front.
    pub fn front_app(&mut self, f: crate::front_app::Front) {
        // (a game's window goes by its game's id, whatever the window calls itself)
        let app = self.games.front_id(&f);
        self.app_windows(&f, &app);
        if f.closed {
            return;
        }
        // a window just opened, in front or not: only a running game's matters here, to
        // its launch screen; what's in front is told as it comes to the front
        if f.opened {
            if self.games.running.iter().any(|r| r.id == app) {
                if let Some(pl) = &self.player {
                    pl.tell(crate::lcd_player::Control::Game(crate::lcd_player::GameSeen::Window));
                }
            }
            return;
        }
        let seen = {
            let (games, seen) = (&self.games, &mut self.front_apps);
            let name = games.name(&app, seen);
            seen.saw_as(&f, &app, || name)
        };
        if f.is_ectomatter() {
            return;
        }
        // a running game's window come to the front: its launch screen can go on
        if self.games.running.iter().any(|r| r.id == app) {
            if let Some(pl) = &self.player {
                pl.tell(crate::lcd_player::Control::Game(crate::lcd_player::GameSeen::Window));
            }
        }
        self.last_front = Some(f);
        let mut changed = false;
        // what the Library says each device does while this is in front (a game
        // with no choice of its own follows Every game's); Don't switch is none
        let wanted = |d: &str| match self.games.library.wanted(&app, d) {
            Some(crate::games::Choice::Profile(p)) => Some(p),
            _ => None,
        };
        let (mouse_wants, kb_wants) = (wanted("sensei310"), wanted("z10").and_then(|p| p.parse::<u8>().ok()));
        let wants: Vec<(String, String, String, Option<String>)> = self
            .hosted
            .devices()
            .into_iter()
            .map(|(plugin, d)| {
                let id = d["device"].as_str().unwrap_or("").to_string();
                let w = wanted(&id);
                (plugin, id, d["name"].as_str().unwrap_or("").to_string(), w)
            })
            .collect();
        // the mouse
        if let Some(store) = &self.mouse.profiles {
            let (active, count) = (store.active, store.list.len());
            match mouse_wants.and_then(|id| store.list.iter().position(|p| p.id == id)) {
                Some(i) if i != active => {
                    self.follow_back[1].get_or_insert(active);
                    self.mouse.choose_profile(i);
                    eprintln!("ectomatter: the mouse follows {app}");
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
            match kb_wants.filter(|n| z10::PROFILE_NUMBERS.contains(n)) {
                Some(n) if n != active => {
                    self.follow_back[0].get_or_insert(active as usize);
                    self.choose_profile(n);
                    eprintln!("ectomatter: the keyboard follows {app}");
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
        // each device a plugin brought in (OpenRGB's lighting): the profile this app wants for
        // it, put on below with the scene's, which comes first (see plugins_to)
        for (plugin, id, name, want) in wants {
            let follow = format!("{plugin}/{id}");
            match want {
                Some(p) if self.following.get(&follow) != Some(&p) => {
                    eprintln!("ectomatter: {name} follows {app}");
                    self.following.insert(follow, p);
                    changed = true;
                }
                Some(_) => {}
                None => {
                    if self.following.remove(&follow).is_some() {
                        changed = true;
                    }
                }
            }
        }
        // the scene: the one this app or game chose while it's in front (a game with none
        // of its own follows Every game's); once it switched, back to the one before
        match self.games.library.wanted(&app, "scene") {
            // (a scene since deleted is no choice)
            Some(crate::games::Choice::Profile(p)) if p == "none" || self.scenes().iter().any(|s| s.id == p) => {
                let want = (p != "none").then_some(p);
                if want != self.scene {
                    if self.scene_back.is_none() {
                        self.scene_back = Some(self.scene.clone());
                    }
                    eprintln!("ectomatter: the scene follows {app}");
                    self.set_scene(want, false);
                    changed = true;
                }
            }
            _ => {
                if let Some(back) = self.scene_back.take() {
                    if back != self.scene {
                        self.set_scene(back, false);
                        changed = true;
                    }
                }
            }
        }
        self.plugins_sync();
        if changed {
            self.show_mouse_lights();
        }
        if changed || seen {
            self.refresh();
        }
    }

    /// The chosen button's jobs of one kind, under the Key detail box's menu field.
    fn open_mouse_menu(&mut self, kind: usize, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let Some(b) = self.mouse.button else { return };
        let now = self.mouse.job(b);
        // (a macro ticked whatever it plays as)
        let same = |a: &serde_json::Value| *a == now || (a.get("macro").is_some() && a["macro"] == now["macro"]);
        let choices = self.mouse.choices_for(kind);
        let mut rows: Vec<MenuRow> = Vec::new();
        // (each row's job, kept as it was offered: the list may change while the menu's
        // open, a plugin's device coming or going; none for a heading or a note)
        let mut picks: Vec<Option<serde_json::Value>> = Vec::new();
        if kind == crate::mouse::DEVICES_KIND {
            // other devices' jobs and the scenes', under their headings, each by its own name,
            // as the keys' Profile page and the Macro Editor list them
            let names = self.job_names();
            let group = |a: &serde_json::Value| crate::jobs::Job::read(a).and_then(|j| names.group(&j)).unwrap_or_default();
            let mut groups: Vec<(String, String)> = Vec::new();
            for (_, a) in &choices {
                if !groups.iter().any(|(k, _)| *k == group(a).0) {
                    groups.push(group(a));
                }
            }
            for (key, head) in groups {
                rows.push(MenuRow { kind: 1, label: head.as_str().into(), ..Default::default() });
                picks.push(None);
                for (label, a) in choices.iter().filter(|(_, a)| group(a).0 == key) {
                    let words = crate::jobs::Job::read(a).map_or_else(|| label.clone(), |j| names.short(&j));
                    rows.push(MenuRow { kind: 0, label: words.into(), tick: same(a), ..Default::default() });
                    picks.push(Some(a.clone()));
                }
            }
        } else {
            for (label, a) in &choices {
                rows.push(MenuRow { kind: 0, label: label.as_str().into(), tag: "".into(), tick: same(a), sub: "".into(), ..Default::default() });
                picks.push(Some(a.clone()));
            }
        }
        if rows.is_empty() && kind == crate::mouse::MACRO_KIND {
            rows.push(MenuRow { kind: 3, label: "No macros yet. Make one in the Macro Editor.".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() });
            picks.push(None);
        }
        if rows.is_empty() && kind == crate::mouse::DEVICES_KIND {
            rows.push(MenuRow { kind: 3, label: "Plug in another device to give this button one of its jobs.".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() });
            picks.push(None);
        }
        self.mouse.menu_picks = picks;
        self.hang_menu(rows, under, anchor);
        self.ui.set_mouse_menu_open(true);
        self.mouse.menu = Some(kind);
    }

    /// The Lighting page's effects and its tile, laid out for the draft. The
    /// pieces are changed in place, so one being used is never rebuilt under
    /// the pointer.
    /// The Lighting page, drawn from the keyboard's description: the effects'
    /// tiles, and the chosen effect's boxes.
    fn refresh_lighting(&mut self) {
        let mode = self.page.effect_id();
        let effects = self.page.effects();
        let ids: Vec<String> = effects.iter().map(|e| e["id"].as_str().unwrap_or("").to_string()).collect();
        let scale = self.scale;
        let wide = pic_wide(self.ui.get_light_fx_w());
        if self.icons.as_ref().map(|(s, w, i, _)| (*s, *w, i)) != Some((scale, wide, &ids)) {
            self.icons = Some((scale, wide, ids.clone(), ids.iter().map(|id| lighting::icon(crate::described::tile(id).0, scale, wide)).collect()));
        }
        let icons = &self.icons.as_ref().unwrap().3;
        let tiles: Vec<EffectItem> = effects
            .iter()
            .zip(icons)
            .map(|(e, icon)| {
                let id = e["id"].as_str().unwrap_or("");
                EffectItem { name: e["label"].as_str().unwrap_or(id).into(), icon: icon.clone(), on: id == mode, motion: crate::described::tile(id).1 }
            })
            .collect();
        self.ui.set_effect_moves(tiles.iter().any(|e| e.on && e.motion != 0));
        show_rows(self.ui.get_effects(), |m| self.ui.set_effects(m), tiles);
        let profile = self.page.profile_name(self.page.editing);
        let look = crate::described::Look {
            kind: "keyboard",
            profile: &profile,
            open: self.pick.as_ref().and_then(|p| match p.piece {
                Some(crate::described::Piece::Effect(k)) => Some((k, p.index)),
                _ => None,
            }),
            painting: self.pick.as_ref().is_some_and(|p| p.ctl == Ctl::Paint),
            menu: self.light_menu,
            hot: self.picture_area(),
            demo: self.page.demo,
            paint: self.paint,
        };
        let (set_w, place_w) = self.light_box_widths();
        let b = crate::described::boxes(&self.page.keys_light(), &self.page.effect_desc(&mode), &self.page.effect_settings(), &look, set_w, place_w);
        // (where and when beside the settings, for an effect with places to light)
        let mut shown = vec![crate::boxes::Laid { title: b.title, least: crate::described::SET_W, h: b.set_h, cells: b.set }];
        if b.kind == 0 && !b.place_title.is_empty() {
            shown.push(crate::boxes::Laid { title: b.place_title, least: crate::described::PLACE_W, h: b.place_h, cells: b.place });
        }
        self.light_boxes.show(shown);
    }

    /// How wide the page's row of boxes made the Lighting tab's settings and its where
    /// and when (their least widths until it has drawn them).
    fn light_box_widths(&self) -> (f32, f32) {
        (self.light_boxes.width(0, crate::described::SET_W), self.light_boxes.width(1, crate::described::PLACE_W))
    }

    /// Whether the Lighting page lights some zones of the keyboard, not all of
    /// it: the effect's zones setting, if it has one, on Some zones.
    fn some_areas(&self) -> bool {
        let p = &self.page;
        // (nothing to say before the keyboard's profiles are read)
        if p.view != View::Lighting || !p.drafts.contains_key(&p.editing) {
            return false;
        }
        let e = p.effect_desc(&p.effect_id());
        let light = p.keys_light();
        crate::described::params(&e).iter().find(|(_, d)| d["type"] == "zones").is_some_and(|(id, d)| !crate::described::whole(&light, crate::described::kept(&p.effect_settings(), id, d).as_u64().unwrap_or(0)))
    }

    /// The zone a key is in, by its place in the keys' light's zones (never the whole keyboard).
    fn area_of(&self, pos: u32) -> Option<usize> {
        let light = self.page.keys_light();
        let n = light["zones"].as_array().map_or(0, |z| z.len());
        (0..n).filter(|&i| light["zones"][i]["whole"] != true).find(|&i| crate::described::zone_bit(&light, i).is_some_and(|bit| self.page.zone_set(bit as u8).is_some_and(|keys| keys.contains(&pos))))
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
        let bit = crate::described::zone_bit(&p.keys_light(), self.hover_area.or_else(|| self.picture_area())?)? as u8;
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
        evga_z10::firmware::describe(v)
    }

    // ------------------------------------------------------------ the sliders' jobs

    /// A dropdown on the Lighting page opened: its choices, under it, as wide as it is.
    fn open_light_menu(&mut self, piece: crate::described::Piece, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        let crate::described::Piece::Effect(k) = piece else { return };
        let Some((id, d, e)) = self.page.described(piece) else { return };
        let rows: Vec<MenuRow> = crate::described::menu_rows(&d, &crate::described::kept(&e, &id, &d)).into_iter().map(|(kind, label, tick)| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: "".into(), ..Default::default() }).collect();
        self.close_menu();
        self.close_picker();
        self.hang_menu(rows, under, anchor);
        self.light_menu = Some(k);
    }

    /// A turn's mark selected: the ways in, under it, the turn's own ticked.
    fn open_way_menu(&mut self, i: usize, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let Some(e) = self.screens.turns(self.page.editing).get(i) else { return };
        let name = self.screens.page_of(e).map_or("The screen".to_string(), |p| p.name.clone());
        let mut rows = vec![MenuRow { kind: 1, label: format!("How {name} comes in").into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() }];
        for t in crate::lcd_page::TRANSITIONS {
            rows.push(MenuRow { kind: 0, label: t.name().into(), tag: "".into(), tick: t == e.comes_in, sub: "".into(), ..Default::default() });
        }
        // the only turn: nothing comes in after it, so its way in waits for another
        if self.screens.turns(self.page.editing).len() == 1 {
            rows.push(MenuRow { kind: 3, label: "Plays once the playlist has a second layout.".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() });
        }
        self.close_menu();
        self.close_picker();
        self.hang_menu(rows, 0.0, anchor);
        self.way_menu = Some(i);
        self.ui.set_pl_menu(i as i32);
    }

    /// A turn's time selected: under it, the times a turn often has, its own among them, and
    /// staying until the layouts are switched; the turn's own ticked.
    fn open_time_menu(&mut self, i: usize, anchor: (f32, f32, f32), aw: f32) {
        if self.just_shut(anchor) {
            return;
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
        let row = |kind: i32, label: String, tick: bool| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: "".into(), ..Default::default() };
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
        self.hang_menu(rows, aw, anchor);
        self.time_menu = Some(TimeMenu { turn: i, aw, picks });
        self.ui.set_pl_timing(i as i32);
    }

    /// The panel's playlist selected: under it, every playlist (the one the
    /// profile plays ticked, each with the profiles that play it), then a new
    /// one, and this one renamed or deleted (never the last).
    fn open_pl_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let n = self.page.editing;
        let mine = self.screens.of_profile(n);
        let row = |kind: i32, label: String, tick: bool, sub: String| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: sub.into(), ..Default::default() };
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
        self.hang_menu(rows, self.ui.get_pl_chooser_w(), anchor);
        self.pl_menu = true;
        self.ui.set_pl_choosing(true);
    }

    /// The onboard layout's chooser selected: under it, the screen off and every screen
    /// layout, each that the keyboard can't show whole saying what stays blank, then the
    /// layout kept, to edit.
    fn open_ob_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let n = self.page.editing;
        let kept = self.screens.onboard_of(n).map(|p| (p.id.clone(), p.name.clone()));
        let row = |kind: i32, label: String, tick: bool, sub: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: sub.into(), ..Default::default() };
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
        self.hang_menu(rows, self.ui.get_ob_chooser_w(), anchor);
        self.ob_menu = true;
        self.ui.set_ob_choosing(true);
    }

    /// Whether the keyboard can keep an onboard layout: from build 5 of Ectomatter's
    /// firmware on (or, for the window's pictures of the tab, ECTOMATTER_ONBOARD).
    fn onboard_ready(&self) -> bool {
        crate::display::has(&self.firmware(), "screen.onboard") || std::env::var_os("ECTOMATTER_ONBOARD").is_some()
    }

    /// What the keyboard knows by itself, for the picture of its onboard layout: the time
    /// (its own clock, which Ectomatter sets from this computer's), and the name and number
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
            None => "The screen and its backlight stay off while Ectomatter isn’t running.".to_string(),
            Some(p) => crate::lcd_page::Gaps::of(p).line().unwrap_or_else(|| "While this tab is open, the keyboard shows this layout instead of the playlist.".to_string()),
        };
        self.ui.set_ob_chooser_w(crate::form::chooser_width(&name).min(OB_CHOOSER_MOST).ceil());
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
        self.hang_menu(rows, 0.0, (x, y, 0.0));
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
        let cat = self.reader.get_or_insert_with(|| crate::lcd_stats::Stats::of(self.hosted.readings(), "page")).catalog();
        crate::screen_edit::From { line: crate::screen_edit::from_line(src, cat, &setup) }
    }

    /// The computer's readings a block can show, for the Shows menu: the sensors checked,
    /// under the devices they read.
    fn shows_groups(&mut self) -> Vec<(String, Vec<(String, crate::lcd_page::Source)>)> {
        let setup = crate::sensors::setup();
        let cat = self.reader.get_or_insert_with(|| crate::lcd_stats::Stats::of(self.hosted.readings(), "page")).catalog();
        crate::screen_edit::computer_groups(cat, &setup)
    }

    /// Settings, at its Sensor Feed box (from the screen editor's From line, or its Shows menu).
    pub fn show_sensors(&mut self) {
        self.ui.set_place("settings".into());
        self.refresh();
        let to = (self.sensor_map.top - 24.0).max(0.0);
        self.ui.set_settings_goto(-1.0);
        self.ui.set_settings_goto(to);
    }

    /// What the Sensor Feed box shows: the computer's sensors and their readings, read again
    /// when a second old; the choices; the tab open, the runs opened, the sensor renamed. A
    /// card that has come in as the main card has its usual sensors checked as it is seen.
    fn sensors_view(&mut self) -> crate::ecto_settings::Sensors {
        if self.sensors_seen.as_ref().is_none_or(|(t, _, _)| t.elapsed() >= Duration::from_secs(1)) {
            let stats = self.reader.get_or_insert_with(|| crate::lcd_stats::Stats::of(self.hosted.readings(), "page"));
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
    fn shown_in(&self, id: &str, cat: &ectomatter_sensors::list::Catalog) -> Vec<String> {
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
        self.ui.set_confirm_body(format!("{} {show} {what}. Unchecked, it shows as unavailable in {them}.", ectomatter_sensors::parts::and_list(&shows)).into());
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
        // (the sensors as the Sensor Feed lists them)
        let cat = self.reader.get_or_insert_with(|| crate::lcd_stats::Stats::of(self.hosted.readings(), "page")).catalog().clone();
        if cat.devices.is_empty() {
            return;
        }
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
                eprintln!("ectomatter: the screen layouts were not saved with their sensors: {e}");
            }
        }
        if let Err(e) = self.settings.set("sensors", setup.to_json()) {
            eprintln!("ectomatter: the sensors were not saved: {e}");
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
        let row = |kind: i32, label: String| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
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
        let (x, y) = at;
        self.hang_menu(rows, 0.0, (x, y, 0.0));
        self.lib_menu = Some(k);
    }

    fn open_add_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
        let mut rows = vec![row(1, "Add a screen layout")];
        for p in &self.screens.library {
            rows.push(row(0, &p.name));
        }
        self.close_menu();
        self.close_picker();
        self.ui.set_menu_marks(false);
        self.hang_menu(rows, 0.0, anchor);
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

    fn open_bind_menu(&mut self, i: usize, anchor: (f32, f32, f32), under: f32) {
        self.close_menu();
        let name = if i == 0 { "dimmer" } else { "volume" };
        let mut rows = vec![MenuRow { kind: 1, label: format!("The {name} controls").into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() }];
        for (v, label) in crate::display::BIND_CHOICES[i] {
            rows.push(MenuRow { kind: 0, label: label.into(), tag: "".into(), tick: self.bind[i] == v, sub: "".into(), ..Default::default() });
        }
        self.hang_menu(rows, under, anchor);
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
    /// they keep working with Ectomatter gone.
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
    /// Ectomatter never throws the volume to wherever the slider sits.
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
        let t = self.light_at.unwrap_or_else(|| self.t0.elapsed().as_secs_f64() * 1000.0);
        let mut lights = self.page.lights(t);
        // (the scene's light, while it lights the keyboard, on each key: but on the
        // keyboard's own Lighting page, where its own lighting is chosen)
        if !self.scene_colors.0.is_empty() && !self.lighting_open().0 {
            for l in &mut lights {
                if let Some(c) = self.scene_colors.0.get(&l.pos) {
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
        self.page.animated() || (self.scene_timer.is_some() && self.scene_moves)
    }

    /// Home's Scenes menu: No scene, the scenes that come with Ectomatter, the
    /// person's own (each with a pencil to change it), and New scene…; the one
    /// on ticked, each with a chip of its colors.
    fn open_scene_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let scenes = self.scenes();
        let on = self.scene.clone();
        let mut rows = vec![MenuRow { kind: 0, label: "No scene".into(), tick: on.is_none(), chip: self.scene_chip(None), chipped: true, ..Default::default() }];
        let mut does = vec![SceneRow::Off];
        let (built, own): (Vec<&crate::scenes::Scene>, Vec<&crate::scenes::Scene>) = scenes.iter().partition(|s| s.built_in);
        for group in [built, own] {
            if group.is_empty() {
                continue;
            }
            rows.push(MenuRow { kind: 2, ..Default::default() });
            does.push(SceneRow::Line);
            for sc in group {
                rows.push(MenuRow { kind: 0, label: sc.name.as_str().into(), tick: on.as_deref() == Some(sc.id.as_str()), chip: self.scene_chip(Some(sc)), chipped: true, edit: !sc.built_in, ..Default::default() });
                does.push(SceneRow::Scene(sc.id.clone()));
            }
        }
        rows.push(MenuRow { kind: 2, ..Default::default() });
        does.push(SceneRow::Line);
        rows.push(MenuRow { kind: 0, label: "New scene…".into(), chip: crate::plus_glyph((14.0 * self.scale).round() as u32), chipped: true, ..Default::default() });
        does.push(SceneRow::New);
        self.ui.set_menu_marks(true);
        self.hang_menu(rows, 0.0, anchor);
        self.scene_menu = Some(does);
    }

    /// Add pad's menu (the makers' sizes, and Draw your own), or the chosen
    /// pad's Size menu (the sizes, its own ticked, and a width and depth to
    /// type), under its button.
    fn open_pad_menu(&mut self, which: PadMenu, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let (rows, _) = self.pad_rows(which);
        // (Add pad ticks nothing, so it keeps no column for ticks)
        self.ui.set_menu_marks(which == PadMenu::Size);
        self.hang_menu(rows, 0.0, anchor);
        self.pad_menu = Some(which);
    }

    /// A pad menu's rows, and what each stands for: the sizes under their
    /// kinds' headings, each with its size in inches; then Draw your own, or
    /// the chosen pad's width and depth to type.
    fn pad_rows(&self, which: PadMenu) -> (Vec<MenuRow>, Vec<Option<PadRow>>) {
        let row = |kind: i32, label: &str, tag: String, tick: bool, sub: &str| MenuRow { kind, label: label.into(), tag: tag.into(), tick, sub: sub.into(), ..Default::default() };
        let now = self.desk.pad_size_mm();
        let at = now.and_then(|(w, h)| crate::desk::standard(w, h));
        let (mut rows, mut what) = (Vec::new(), Vec::new());
        let mut kind = "";
        for (k, (g, label, w, h)) in crate::desk::SIZES.iter().enumerate() {
            if *g != kind {
                rows.push(row(1, g, String::new(), false, ""));
                what.push(None);
                kind = g;
            }
            rows.push(row(0, label, String::new(), which == PadMenu::Size && at == Some(k), &crate::desk::size_words(*w, *h)));
            what.push(Some(PadRow::Size(k)));
        }
        rows.push(row(2, "", String::new(), false, ""));
        what.push(None);
        match which {
            PadMenu::Add => {
                rows.push(row(0, "Draw your own", String::new(), false, ""));
                what.push(Some(PadRow::Draw));
            }
            PadMenu::Size => {
                let (w, h) = now.unwrap_or((0.0, 0.0));
                rows.push(row(1, "Your own size", String::new(), false, ""));
                what.push(None);
                rows.push(row(6, "Width", crate::desk::inches(w), false, "in"));
                what.push(Some(PadRow::Width));
                rows.push(row(6, "Depth", crate::desk::inches(h), false, "in"));
                what.push(Some(PadRow::Depth));
            }
        }
        (rows, what)
    }

    /// A field in the open menu typed in and set: the chosen pad's width or
    /// depth, in inches (or in mm or cm, said so), the menu staying open for
    /// the other.
    fn menu_field(&mut self, i: usize, t: &str) {
        let Some(which) = self.pad_menu else { return };
        let mm = crate::desk::typed_mm(t);
        match (self.pad_rows(which).1.get(i).copied().flatten(), mm) {
            (Some(PadRow::Width), Some(mm)) => self.desk.set_pad_size(Some(mm), None),
            (Some(PadRow::Depth), Some(mm)) => self.desk.set_pad_size(None, Some(mm)),
            _ => {}
        }
        // (what it is now, in its rows: a size it took, or what it was, for words it couldn't)
        let (rows, _) = self.pad_rows(which);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        self.show_desk();
    }

    // ------------------------------------------------------------ scenes

    /// Every scene: the two that come with Ectomatter, then the person's own.
    pub fn scenes(&self) -> Vec<crate::scenes::Scene> {
        crate::scenes::all(&self.settings.get("scenes").unwrap_or_default())
    }

    /// The scene on, if any (chosen by hand, or by a game or app in front).
    pub fn scene_on(&self) -> Option<crate::scenes::Scene> {
        let id = self.scene.as_ref()?;
        self.scenes().into_iter().find(|s| &s.id == id)
    }

    /// The scene on, by its id.
    pub fn scene_id(&self) -> Option<String> {
        self.scene.clone()
    }

    /// What the desk plays now: the scene form's scene while the form is open
    /// (the devices show it as it's set), else the scene on.
    fn playing(&self) -> Option<crate::scenes::Scene> {
        match &self.scene_form {
            Some(f) => Some(f.scene()),
            None => self.scene_on(),
        }
    }

    /// A scene turned on, or none (No scene): by hand, kept to come on again
    /// when Ectomatter next starts; or for a game or app in front, not kept.
    pub fn set_scene(&mut self, id: Option<String>, by_hand: bool) {
        let id = id.filter(|i| self.scenes().iter().any(|s| &s.id == i));
        if by_hand {
            self.scene_back = None;
            if let Err(err) = self.settings.set("scene", id.as_ref().map_or(serde_json::Value::Null, |i| serde_json::json!(i))) {
                self.status(format!("The scene was not saved: {err}"), 2);
            }
        }
        self.scene = id;
        self.scene_play();
    }

    /// The desk lit as the scene playing has it: its frames from now (one at
    /// once), each plugin's device on the scene's profile; with none playing,
    /// each device's own lighting handed back to it.
    fn scene_play(&mut self) {
        // (handed back for Ectomatter quitting: nothing lights the devices again)
        if self.scene_quit_done {
            return;
        }
        let playing = self.playing();
        if playing.is_none() {
            self.scene_timer = None;
            self.scene_hand_back();
        } else {
            if self.scene_timer.is_none() {
                self.scene_t0 = Instant::now();
                let t = slint::Timer::default();
                let asks = self.asks.clone();
                // (the keyboard takes 25 frames a second)
                t.start(slint::TimerMode::Repeated, Duration::from_millis(40), move || asks.borrow_mut().push(Ask::SceneFrame));
                self.scene_timer = Some(t);
            }
            self.scene_frame();
        }
        self.scene_moves = playing.as_ref().is_some_and(|s| s.lights.moves());
        self.plugins_sync();
        self.show_scene_button();
        self.show_mouse_lights();
    }

    /// Each device's own lighting again, where the scene was sending it its
    /// part: the keyboard out of its direct colors, the mouse's lights as it
    /// keeps them.
    fn scene_hand_back(&mut self) {
        if self.scene_claim.0 {
            self.ask_kb(device::Ask::Preview(serde_json::json!({ "stop": true, "tab": "native", "claim": true })));
        }
        if self.scene_claim.1 {
            self.mouse.hand_back();
        }
        self.scene_claim = (false, false);
        self.scene_colors = Default::default();
        self.scene_mouse = (None, [[0; 3]; 2]);
        self.desk_aside = (false, false);
        self.last_lights = None;
    }

    /// The scene off for Ectomatter quitting, each device its own again; kept
    /// as it was, to come on again next time.
    pub fn scene_quit(&mut self) {
        self.scene_timer = None;
        self.scene_form = None;
        self.scene_hand_back();
        if !crate::desk::offscreen() && !self.scene_quit_done {
            // (each plugin's device Ectomatter switched back on its own, after any call
            // still waiting its turn; up to three seconds for them, then Ectomatter goes)
            self.plugins_to(None, true);
            self.plugin_calls = None;
            if let Some(done) = self.plugin_done.take() {
                let _ = done.recv_timeout(Duration::from_secs(3));
            }
        }
        self.scene_quit_done = true;
    }

    /// Escape on the scene form: its open dropdown or the color picker shut
    /// first, else the form, unkept.
    fn scene_form_escape(&mut self) {
        if self.ui.get_menu_open() || self.pick.is_some() {
            self.close_menu();
            self.close_picker();
        } else {
            self.close_scene_form(false);
        }
    }

    /// The scene form shut as Ectomatter closes to the tray, unkept: the scene
    /// on plays again, not the one being set.
    pub fn cancel_scene_form(&mut self) {
        if self.scene_form.is_some() {
            self.close_scene_form(false);
        }
    }

    /// Each device a plugin brought in (OpenRGB's lighting) on the profile it
    /// plays now: the scene playing's, else the one the app in front chose
    /// (`following`), else back on what it was on before Ectomatter switched
    /// it. A device is sent one call when that changes, every call in the
    /// order made, on a thread of their own; a device gone is forgotten, to be
    /// put on its profile again when it's back. Offscreen, nothing is sent.
    fn plugins_sync(&mut self) {
        if crate::desk::offscreen() || self.scene_quit_done {
            return;
        }
        let scene = self.playing();
        self.plugins_to(scene.as_ref(), false);
    }

    /// The plugins' devices put on what `scene` and the app in front have them
    /// play; quitting, each one Ectomatter switched back on its own.
    fn plugins_to(&mut self, scene: Option<&crate::scenes::Scene>, quitting: bool) {
        let devices = self.hosted.devices();
        let live: std::collections::HashSet<String> = devices.iter().map(|(plugin, d)| format!("{plugin}/{}", d["device"].as_str().unwrap_or(""))).collect();
        self.plugin_applied.retain(|k, _| live.contains(k));
        for (plugin, d) in devices {
            let id = d["device"].as_str().unwrap_or("").to_string();
            let key = format!("{plugin}/{id}");
            let want = if quitting { None } else { scene.and_then(|s| s.plugins.iter().find(|(dev, _)| *dev == id).map(|(_, p)| p.clone())).or_else(|| self.following.get(&key).cloned()) };
            // (a profile chosen by hand stays while the scene playing and the app in front's
            // choice are what they were then; once either changes, what's wanted goes on)
            let why = (scene.map(|s| s.id.clone()), self.following.get(&key).cloned());
            let mut force = quitting && self.plugin_hand.contains_key(&key);
            if let Some(then) = self.plugin_hand.get(&key) {
                if *then == why && !quitting {
                    continue;
                }
                self.plugin_hand.remove(&key);
                force = true;
            }
            if !force && want == self.plugin_applied.get(&key).cloned() {
                continue;
            }
            let Some(core) = self.hosted.get(&plugin).map(|p| p.core.clone()) else { continue };
            let name = d["name"].as_str().unwrap_or("").to_string();
            let (method, params) = match &want {
                Some(p) => ("profiles/activate", serde_json::json!({ "device": id, "profile": p })),
                None => ("profiles/restore", serde_json::json!({ "device": id })),
            };
            eprintln!("ectomatter: {name}: {method}");
            match want {
                Some(p) => self.plugin_applied.insert(key, p),
                None => self.plugin_applied.remove(&key),
            };
            self.plugin_call(core, method, params, name);
        }
    }

    /// A call to a plugin's device, on the thread they all go out on, one after
    /// another (its plugin may take a moment to reach the program it goes through).
    fn plugin_call(&mut self, core: crate::core_host::Core, method: &'static str, params: serde_json::Value, name: String) {
        if self.plugin_calls.is_none() {
            let (tx, rx) = mpsc::channel::<(crate::core_host::Core, &'static str, serde_json::Value, String)>();
            let (done_tx, done_rx) = mpsc::channel::<()>();
            let _ = std::thread::Builder::new().name("plugin devices".into()).spawn(move || {
                for (core, method, params, name) in rx {
                    if let Err(e) = core.call(method, &params) {
                        eprintln!("ectomatter: {name} didn’t switch: {e}");
                    }
                }
                let _ = done_tx.send(());
            });
            self.plugin_calls = Some(tx);
            self.plugin_done = Some(done_rx);
        }
        if let Some(tx) = &self.plugin_calls {
            let _ = tx.send((core, method, params, name));
        }
    }

    /// A plugin's device switched to one of its profiles by hand (a control's
    /// job): through its plugin, in turn with the scenes' and the apps'
    /// switches; the person's own choice from then on (profiles/choose).
    fn plugin_choose(&mut self, device: &str, profile: &str) {
        if self.scene_quit_done || crate::desk::offscreen() {
            return;
        }
        let Some((plugin, d)) = self.hosted.devices().into_iter().find(|(_, d)| d["device"].as_str() == Some(device)) else { return };
        let name = d["name"].as_str().unwrap_or("").to_string();
        if d["capabilities"]["profiles"]["choose"] != true {
            eprintln!("ectomatter: {name} can’t be switched by hand");
            return;
        }
        let Some(core) = self.hosted.get(&plugin).map(|p| p.core.clone()) else { return };
        let key = format!("{plugin}/{device}");
        let scene = self.playing();
        self.plugin_hand.insert(key.clone(), (scene.map(|s| s.id), self.following.get(&key).cloned()));
        eprintln!("ectomatter: {name}: profiles/choose");
        self.plugin_call(core, "profiles/choose", serde_json::json!({ "device": device, "profile": profile }), name);
    }

    /// Home's Scenes button: the scene on, its name and its chip (with none
    /// on, the button says Scenes).
    fn show_scene_button(&mut self) {
        let on = self.scene_on();
        self.ui.set_scene_name(on.as_ref().map_or_else(String::new, |s| s.name.clone()).into());
        self.ui.set_scene_chip(self.scene_chip(on.as_ref()));
    }

    /// The devices on Home's desk as a scene's pictures lay them, left to
    /// right: the keyboard and the mouse by their keys, each where it runs
    /// across the desk.
    fn scene_spans(&self) -> Vec<crate::scenes::Span> {
        let mut out: Vec<crate::scenes::Span> = self
            .desk
            .items
            .iter()
            .filter_map(|it| {
                let key = match it.id.as_str() {
                    "z10" => "z10",
                    "mouse" => "sensei310",
                    _ => return None,
                };
                Some(crate::scenes::Span { key: key.into(), x0: it.x / crate::desk::PER_MM, x1: (it.x + it.w) / crate::desk::PER_MM })
            })
            .collect();
        out.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        out
    }

    /// A scene's chip at the window's scale (none's, an empty outline).
    fn scene_chip(&self, s: Option<&crate::scenes::Scene>) -> slint::Image {
        let px = (14.0 * self.scale).round().max(14.0) as u32;
        let rgba = crate::scenes::chip(s, &self.scene_spans(), px);
        let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(px, px);
        buf.make_mut_bytes().copy_from_slice(&rgba);
        slint::Image::from_rgba8_premultiplied(buf)
    }

    /// Each scene's chip, No scene's first, in the order a Library page's
    /// Scene row lists them.
    pub fn scene_chips(&self) -> Vec<slint::Image> {
        std::iter::once(self.scene_chip(None)).chain(self.scenes().iter().map(|s| self.scene_chip(Some(s)))).collect()
    }

    /// The scene a device's Lighting page is out of, by its name: the scene on,
    /// while it lights that device (by its key, `z10` or `sensei310`).
    fn scene_lighting(&self, device: &str) -> Option<String> {
        let s = self.scene_on()?;
        matches!(s.lights.part(device), crate::scenes::Part::Look(_)).then_some(s.name)
    }

    // ------------------------------------------------------------ the scene form

    /// The scene form opened over the page: New scene, or one of the person's
    /// own scenes to change. The devices show the scene as it's set.
    pub fn open_scene_form(&mut self, editing: Option<String>) {
        use crate::scenes::{Effect, Lights, Look, Part};
        self.close_menu();
        self.close_picker();
        let start = Part::Look(Look::new(Effect::Static));
        let keys = ["z10", "sensei310"];
        let mut form = SceneForm { editing: None, name: String::new(), desk: true, look: Look::new(Effect::Wave), parts: keys.iter().map(|k| (k.to_string(), start.clone())).collect(), plugins: Vec::new() };
        if let Some(sc) = editing.and_then(|id| self.scenes().into_iter().find(|s| s.id == id && !s.built_in)) {
            form.editing = Some(sc.id.clone());
            form.name = sc.name.clone();
            match &sc.lights {
                Lights::Desk(l) => form.look = l.clone(),
                Lights::Each(_) => {
                    form.desk = false;
                    form.parts = keys.iter().map(|k| (k.to_string(), sc.lights.part(k))).collect();
                }
            }
            form.plugins = sc.plugins.clone();
        }
        self.scene_form = Some(form);
        self.ui.set_scene_form(true);
        self.show_scene_form();
        self.scene_play();
    }

    /// The scene form closed: Cancel (the scene on plays again), or Create
    /// scene (kept, and on) or Save (kept; on again if it was).
    fn close_scene_form(&mut self, keep: bool) {
        let Some(form) = self.scene_form.take() else { return };
        self.close_menu();
        self.close_picker();
        self.ui.set_scene_form(false);
        // (the Name box's typing let go of: it goes without being left)
        slint::ComponentHandle::global::<crate::Typing>(&self.ui).set_on(false);
        if keep {
            let all = self.scenes();
            let mut sc = form.scene();
            let before = form.editing.as_ref().and_then(|id| all.iter().find(|s| &s.id == id)).map(|s| s.name.clone());
            sc.name = match (form.name.trim(), before) {
                ("", Some(was)) => was,
                ("", None) => (1..).map(|n| format!("Scene {n}")).find(|n| !all.iter().any(|s| &s.name == n)).unwrap_or_default(),
                (n, _) => n.to_string(),
            };
            let mut kept: Vec<serde_json::Value> = self.settings.get("scenes").and_then(|v| v.as_array().cloned()).unwrap_or_default();
            match &form.editing {
                Some(id) => {
                    if let Some(v) = kept.iter_mut().find(|v| v["id"].as_str() == Some(id)) {
                        *v = crate::scenes::to_json(&sc);
                    }
                }
                None => {
                    // (an id never used before, not even by a scene since deleted)
                    let made = self.settings.get("scenes_made").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    sc.id = crate::scenes::new_id(&all, made);
                    let _ = self.settings.set("scenes_made", serde_json::json!(made.max(crate::scenes::id_number(&sc.id)) as u64));
                    kept.push(crate::scenes::to_json(&sc));
                }
            }
            if let Err(err) = self.settings.set("scenes", serde_json::Value::Array(kept)) {
                self.status(format!("The scene was not saved: {err}"), 2);
            }
            if form.editing.is_none() {
                self.set_scene(Some(sc.id), true);
            } else {
                self.scene_play();
            }
        } else {
            self.scene_play();
        }
        self.refresh();
    }

    /// The scene being changed deleted: the games and apps that turned it on
    /// go back to Every game's choice (or none), and it's off if it was on.
    fn delete_scene(&mut self) {
        let Some(id) = self.scene_form.as_ref().and_then(|f| f.editing.clone()) else { return };
        self.scene_form = None;
        self.close_menu();
        self.close_picker();
        self.ui.set_scene_form(false);
        slint::ComponentHandle::global::<crate::Typing>(&self.ui).set_on(false);
        let mut kept: Vec<serde_json::Value> = self.settings.get("scenes").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        kept.retain(|v| v["id"].as_str() != Some(id.as_str()));
        let _ = self.settings.set("scenes", serde_json::Value::Array(kept));
        self.games.library.forget_scene(&id);
        if self.scene_back.as_ref().is_some_and(|b| b.as_deref() == Some(id.as_str())) {
            self.scene_back = Some(None);
        }
        if self.scene.as_deref() == Some(id.as_str()) {
            match self.scene_back.take() {
                // (a game or app had it on: the scene before it, as when that app leaves the front)
                Some(back) => self.set_scene(back, false),
                None => self.set_scene(None, true),
            }
        } else {
            self.scene_play();
        }
        self.refresh();
    }

    /// What the form says under its title: what a scene is for, or, changing
    /// one that games or apps turn on, which (so deleting it surprises none).
    fn scene_form_sub(&self) -> String {
        let plain = "Light the whole desk with one effect, or give each device its own.".to_string();
        let Some(id) = self.scene_form.as_ref().and_then(|f| f.editing.clone()) else { return plain };
        let tied = self.games.library.tied("scene", &id);
        let names: Vec<String> = tied.iter().map(|a| if a == crate::games::EVERY { "Every game".to_string() } else { self.games.name(a, &self.front_apps) }).collect();
        match names.as_slice() {
            [] => plain,
            [one] if tied[0] == crate::games::EVERY => format!("{one} turns this scene on while a game is in front."),
            [one] if self.games.library.is_game(&tied[0]) => format!("{one} turns this scene on while the game is in front."),
            [one] => format!("{one} turns this scene on while the app is in front."),
            [rest @ .., last] => format!("{} and {last} turn this scene on while they’re in front.", rest.join(", ")),
        }
    }

    /// The devices the form gives a part each, as Home's desk has them, left
    /// to right: each one's key, name and glyph's style.
    fn scene_form_devices(&self) -> Vec<(String, String, i32)> {
        let named: HashMap<String, String> = self.devices_in_order().into_iter().map(|(place, name, ..)| (place, name)).collect();
        self.scene_spans()
            .into_iter()
            .map(|sp| {
                let (place, style) = if sp.key == "z10" { ("z10", 0) } else { ("mouse", 1) };
                let name = named.get(place).cloned().unwrap_or_else(|| if place == "z10" { "Keyboard".to_string() } else { "Mouse".to_string() });
                (sp.key, name, style)
            })
            .collect()
    }

    /// The plugins' devices a scene can give a profile (OpenRGB's): each one's
    /// id, name, glyph's style and profiles (by id and name).
    fn scene_form_plugins(&self) -> Vec<(String, String, i32, Vec<(String, String)>)> {
        self.hosted_devices().into_iter().filter(|d| !d.profiles.is_empty()).map(|d| (d.key, d.name, d.glyph.map_or(0, |g| 2 + g as i32), d.profiles)).collect()
    }

    /// The form drawn as it is: its title and words, its pieces (the cells
    /// under the preview), the effect tiles and the preview itself.
    fn show_scene_form(&mut self) {
        use crate::lighting::{cell, id, kind, style, text_cell};
        use crate::scenes::{Effect, Part};
        use crate::tokens::{line, part, size, space};
        let Some(form) = &self.scene_form else { return };
        // (the dialog's room inside its padding; a stacked form: each label over what it
        // labels, Space.label between, Space.pad between one and the next; two columns of
        // it Space.margin apart)
        const W: f32 = size::DIALOG - 2.0 * space::MARGIN;
        const CTRL: f32 = size::CONTROL;
        const HALF: f32 = (W - space::MARGIN) / 2.0;
        // (a device's row: its effect's dropdown, its colors after it)
        const PART_DROP: f32 = 190.0;
        let mut cells: Vec<Cell> = Vec::new();
        let mut y = 0.0;
        // the preview's words: each device under its part of the strip
        let spans = self.scene_spans();
        let devices = self.scene_form_devices();
        let room = W - part::SPAN_GAP * spans.len().saturating_sub(1) as f32;
        let mut x = 0.0;
        for (share, (_, name, _)) in crate::scenes::shares(&spans).into_iter().zip(&devices) {
            let w = share * room;
            cells.push(text_cell(style::NOTE, x, y, w - 2.0 * part::SPAN_GAP, line::NOTE, name));
            x += w + part::SPAN_GAP;
        }
        y += line::NOTE + space::PAD;
        // Name
        cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, "Name"));
        y += line::TEXT + space::LABEL;
        cells.push(Cell { text: form.name.as_str().into(), fill: "Name this scene".into(), style: 9, id: id(Ctl::SceneName), ..cell(kind::FIELD, 0.0, y, W, CTRL) });
        y += CTRL + space::PAD;
        // Lights: the whole desk, or each device
        cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, "Lights"));
        y += line::TEXT + space::LABEL;
        let seg = |cells: &mut Vec<Cell>, x0: f32, y: f32, c: Ctl, words: &[&str], on: usize| {
            let opts: Vec<(&str, &str)> = words.iter().map(|w| (*w, "")).collect();
            crate::form::switch(cells, x0, y, None, &opts, Some(on), |j| (id(c), j as i32, 0));
        };
        seg(&mut cells, 0.0, y, Ctl::SceneLights, &["Whole desk", "Each device"], if form.desk { 0 } else { 1 });
        y += CTRL + space::TIGHT;
        let order: Vec<&str> = devices.iter().map(|(_, _, glyph)| if *glyph == 0 { "the keyboard" } else { "the mouse" }).collect();
        let note = if form.desk {
            match order.as_slice() {
                [a, b] => format!("One effect crosses {a}, then {b}, in the order they sit on Home."),
                _ => "One effect crosses the whole desk, in the order the devices sit on Home.".to_string(),
            }
        } else {
            "Each device plays an effect of its own. Leave a device as it is to keep its profile’s lighting.".to_string()
        };
        y = crate::form::note_lines(&mut cells, 0.0, y, W, style::NOTE, &note) + space::PAD;
        // the colors as dots, Space.inline apart, each one's code Space.tight after it and
        // Space.row before the next, as a box's colors are
        let dots = |cells: &mut Vec<Cell>, x0: f32, y: f32, colors: &[[u8; 3]], base: usize, codes: bool| -> f32 {
            let mut x = x0;
            for (k, c) in colors.iter().enumerate() {
                cells.push(Cell { color: crate::tokens::color(*c), id: id(Ctl::SceneColor), index: (base + k) as i32, ..cell(kind::COLOR_DOT, x, y + (CTRL - size::SWATCH) / 2.0, size::SWATCH, size::SWATCH) });
                if codes {
                    let hex = lighting::hex_of(*c);
                    let w = crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CAPTION, &hex).ceil() + 2.0;
                    cells.push(text_cell(style::CODE, x + size::SWATCH + space::TIGHT, y, w, CTRL, &hex));
                    x += size::SWATCH + space::TIGHT + w + space::ROW;
                } else {
                    x += size::SWATCH + space::INLINE;
                }
            }
            x
        };
        let mut tiles_y = 0.0;
        if form.desk {
            let l = &form.look;
            cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, "Effect"));
            y += line::TEXT + space::LABEL;
            tiles_y = y;
            y += size::TILE + space::PAD;
            if l.effect.colors() > 0 || l.effect == Effect::Wave {
                if l.effect.colors() > 0 {
                    cells.push(text_cell(style::LABEL, 0.0, y, HALF, line::TEXT, "Colors"));
                }
                if l.effect == Effect::Wave {
                    cells.push(text_cell(style::LABEL, HALF + space::MARGIN, y, HALF, line::TEXT, "Direction"));
                }
                y += line::TEXT + space::LABEL;
                dots(&mut cells, 0.0, y, &l.colors, 0, true);
                if l.effect == Effect::Wave {
                    seg(&mut cells, HALF + space::MARGIN, y, Ctl::SceneWay, &["Left to right", "Right to left"], l.reverse as usize);
                }
                y += CTRL + space::PAD;
            }
            if l.effect != Effect::Static {
                cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, "Speed"));
                let words = crate::scenes::speed_words(l.speed);
                let ww = crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, words).ceil() + 2.0;
                cells.push(text_cell(style::NOTE, W - ww, y, ww, line::TEXT, words));
                y += line::TEXT + space::LABEL;
                cells.push(Cell { value: l.speed, color: crate::tokens::color(crate::tokens::ink::SOFT), style: 3, id: id(Ctl::SceneSpeed), ..cell(kind::FILLED, 0.0, y, W, size::SLIDER) });
                y += size::SLIDER + space::PAD;
            }
            for (k, (dev, name, _, profiles)) in self.scene_form_plugins().iter().enumerate() {
                cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, name));
                y += line::TEXT + space::LABEL;
                let shown = form.plugins.iter().find(|(d, _)| d == dev).and_then(|(_, p)| profiles.iter().find(|(r, _)| r == p).map(|(_, n)| n.clone())).unwrap_or_else(|| "Leave as it is".into());
                cells.push(Cell { text: shown.as_str().into(), on: self.scene_form_menu == Some(FormMenu::Plugin(dev.clone())), id: id(Ctl::ScenePlugin), index: k as i32, ..cell(kind::DROP, 0.0, y, HALF, CTRL) });
                y += CTRL + space::TIGHT;
                y = crate::form::note_lines(&mut cells, 0.0, y, W, style::NOTE, &format!("{name} lights its devices from its own profiles. Choose one for this scene.")) + space::PAD;
            }
            y -= space::PAD;
        } else {
            // a row for each device: its glyph, its name, its effect and its colors
            let mut first = true;
            for (k, (key, name, glyph)) in devices.iter().enumerate() {
                if !first {
                    cells.push(crate::form::rule(0.0, y, W));
                }
                first = false;
                cells.push(Cell { style: *glyph, ..cell(kind::DEVICE, 0.0, y + space::ROW, CTRL, CTRL) });
                cells.push(text_cell(style::LABEL, CTRL + space::ROW, y + space::ROW, size::LABEL - CTRL - space::ROW, CTRL, name));
                let part = form.parts.iter().find(|(d, _)| d == key).map_or(Part::AsIs, |(_, p)| p.clone());
                let word = match &part {
                    Part::AsIs => "Leave as it is",
                    Part::Look(l) => l.effect.label(),
                };
                let dx = size::LABEL + space::PAD;
                cells.push(Cell { text: word.into(), on: self.scene_form_menu == Some(FormMenu::Part(key.clone())), id: id(Ctl::ScenePart), index: k as i32, ..cell(kind::DROP, dx, y + space::ROW, PART_DROP, CTRL) });
                if let Part::Look(l) = &part {
                    dots(&mut cells, dx + PART_DROP + space::ROW, y + space::ROW, &l.colors, 100 * (k + 1), l.colors.len() == 1);
                }
                y += space::ROW + CTRL + space::ROW;
            }
            for (k, (dev, name, glyph, profiles)) in self.scene_form_plugins().iter().enumerate() {
                cells.push(crate::form::rule(0.0, y, W));
                cells.push(Cell { style: *glyph, ..cell(kind::DEVICE, 0.0, y + space::ROW, CTRL, CTRL) });
                cells.push(text_cell(style::LABEL, CTRL + space::ROW, y + space::ROW, size::LABEL - CTRL - space::ROW, CTRL, name));
                let shown = form.plugins.iter().find(|(d, _)| d == dev).and_then(|(_, p)| profiles.iter().find(|(r, _)| r == p).map(|(_, n)| n.clone())).unwrap_or_else(|| "Leave as it is".into());
                cells.push(Cell { text: shown.as_str().into(), on: self.scene_form_menu == Some(FormMenu::Plugin(dev.clone())), id: id(Ctl::ScenePlugin), index: k as i32, ..cell(kind::DROP, size::LABEL + space::PAD, y + space::ROW, PART_DROP, CTRL) });
                y += space::ROW + CTRL + space::ROW;
            }
        }
        let editing = form.editing.is_some();
        let moves = form.scene().lights.moves();
        let tiles = form.desk.then(|| self.scene_tiles());
        self.scene_moves = moves;
        self.ui.set_scene_form_title(if editing { "Edit scene" } else { "New scene" }.into());
        self.ui.set_scene_form_sub(self.scene_form_sub().into());
        self.ui.set_scene_form_edit(editing);
        self.ui.set_scene_form_h(y);
        self.ui.set_scene_tiles_y(tiles_y);
        self.ui.set_scene_tiles_on(tiles.is_some());
        show_rows(self.ui.get_scene_tiles(), |m| self.ui.set_scene_tiles(m), tiles.unwrap_or_default());
        sync_cells(&self.scene_cells, cells);
        self.show_scene_strip(self.scene_t0.elapsed().as_secs_f32());
    }

    /// The scene form's effect tiles: each effect in the form's colors, the one chosen ticked.
    fn scene_tiles(&self) -> Vec<EffectItem> {
        let Some(form) = &self.scene_form else { return Vec::new() };
        crate::scenes::EFFECTS
            .iter()
            .map(|(e, _, label)| {
                let l = form.look.as_effect(*e);
                let pic = crate::tokens::size::SCENE_TILE - 2.0 * crate::tokens::part::TILE_INSET;
                let (w, h) = ((pic * self.scale).round() as u32, (crate::tokens::part::TILE_PIC * self.scale).round() as u32);
                let across = if *e == crate::scenes::Effect::Wave { 2 } else { 1 };
                let rgba = crate::scenes::tile(&l, w * across, h);
                let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w * across, h);
                buf.make_mut_bytes().copy_from_slice(&rgba);
                let motion = match e {
                    crate::scenes::Effect::Breathing => 1,
                    crate::scenes::Effect::Wave => 3,
                    _ => 0,
                };
                EffectItem { name: (*label).into(), icon: slint::Image::from_rgba8(buf), on: *e == form.look.effect, motion }
            })
            .collect()
    }

    /// The form's preview: the scene across the desk now.
    fn show_scene_strip(&mut self, t: f32) {
        let Some(form) = &self.scene_form else { return };
        let room = crate::tokens::size::DIALOG - 2.0 * crate::tokens::space::MARGIN;
        let (w, h, gap) = ((room * self.scale).round() as u32, (crate::tokens::size::PREVIEW * self.scale).round() as u32, (crate::tokens::part::SPAN_GAP * self.scale).round() as u32);
        let rgba = crate::scenes::strip(&form.scene().lights, &self.scene_spans(), w, h, gap, t);
        let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
        buf.make_mut_bytes().copy_from_slice(&rgba);
        self.ui.set_scene_strip(slint::Image::from_rgba8(buf));
    }

    /// One of the form's dropdowns opened, under it: a device's effects (or
    /// Leave as it is), or a plugin device's profiles.
    fn open_scene_form_menu(&mut self, m: FormMenu, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let Some(form) = &self.scene_form else { return };
        let row = |label: &str, tick: bool| MenuRow { kind: 0, label: label.into(), tick, ..Default::default() };
        let rows = match &m {
            FormMenu::Part(key) => {
                let now = form.parts.iter().find(|(d, _)| d == key).map(|(_, p)| p.clone());
                let effect = match &now {
                    Some(crate::scenes::Part::Look(l)) => Some(l.effect),
                    _ => None,
                };
                std::iter::once(row("Leave as it is", effect.is_none())).chain(std::iter::once(MenuRow { kind: 2, ..Default::default() })).chain(crate::scenes::EFFECTS.iter().map(|(e, _, w)| row(w, effect == Some(*e)))).collect()
            }
            FormMenu::Plugin(dev) => {
                let now = form.plugins.iter().find(|(d, _)| d == dev).map(|(_, p)| p.clone());
                let profiles = self.scene_form_plugins().into_iter().find(|(d, ..)| d == dev).map(|(.., p)| p).unwrap_or_default();
                std::iter::once(row("Leave as it is", now.is_none())).chain(std::iter::once(MenuRow { kind: 2, ..Default::default() })).chain(profiles.iter().map(|(r, n)| row(n, now.as_deref() == Some(r.as_str())))).collect()
            }
        };
        self.ui.set_menu_marks(true);
        self.hang_menu(rows, under, anchor);
        self.scene_form_menu = Some(m);
        self.show_scene_form();
    }

    /// A row of the form's open dropdown chosen.
    fn scene_form_chose(&mut self, m: &FormMenu, i: usize) {
        use crate::scenes::{Look, Part, EFFECTS};
        let profiles = match m {
            FormMenu::Plugin(dev) => self.scene_form_plugins().into_iter().find(|(d, ..)| d == dev).map(|(.., p)| p).unwrap_or_default(),
            FormMenu::Part(_) => Vec::new(),
        };
        let Some(form) = &mut self.scene_form else { return };
        match m {
            FormMenu::Part(key) => {
                let new = match i {
                    0 => Some(Part::AsIs),
                    i if i >= 2 => EFFECTS.get(i - 2).map(|(e, ..)| {
                        let was = form.parts.iter().find(|(d, _)| d == key).and_then(|(_, p)| if let Part::Look(l) = p { Some(l.clone()) } else { None });
                        Part::Look(was.map_or_else(|| Look::new(*e), |l| l.as_effect(*e)))
                    }),
                    _ => None,
                };
                if let Some(p) = new {
                    match form.parts.iter_mut().find(|(d, _)| d == key) {
                        Some(x) => x.1 = p,
                        None => form.parts.push((key.clone(), p)),
                    }
                }
            }
            FormMenu::Plugin(dev) => {
                form.plugins.retain(|(d, _)| d != dev);
                if i >= 2 {
                    if let Some((r, _)) = profiles.get(i - 2) {
                        form.plugins.push((dev.clone(), r.clone()));
                    }
                }
            }
        }
        self.show_scene_form();
        self.scene_play();
    }

    /// A color of the form's, by its place (the whole desk's 0 and 1; a
    /// device's, 100 for each row down, and its own place).
    fn scene_form_color(&self, index: usize) -> Option<[u8; 3]> {
        let f = self.scene_form.as_ref()?;
        if index < 100 {
            return f.look.colors.get(index).copied();
        }
        let (row, k) = (index / 100 - 1, index % 100);
        let key = self.scene_form_devices().get(row)?.0.clone();
        match f.parts.iter().find(|(d, _)| *d == key)? {
            (_, crate::scenes::Part::Look(l)) => l.colors.get(k).copied(),
            _ => None,
        }
    }

    fn set_scene_form_color(&mut self, index: usize, rgb: [u8; 3]) {
        let key = (index >= 100).then(|| self.scene_form_devices().get(index / 100 - 1).map(|d| d.0.clone())).flatten();
        if index >= 100 && key.is_none() {
            return;
        }
        let Some(f) = &mut self.scene_form else { return };
        let colors = match &key {
            None => Some(&mut f.look.colors),
            Some(k) => f.parts.iter_mut().find(|(d, _)| d == k).and_then(|(_, p)| if let crate::scenes::Part::Look(l) = p { Some(&mut l.colors) } else { None }),
        };
        if let Some(c) = colors.and_then(|c| c.get_mut(index % 100)) {
            *c = rgb;
        }
        self.show_scene_form();
    }

    /// Whether each device's own Lighting page shows now, the keyboard's and
    /// the mouse's: there the device plays its own lighting, the one being
    /// chosen, on itself and on its picture, out of the desk's light.
    fn lighting_open(&self) -> (bool, bool) {
        let place = self.ui.get_place();
        (place == "z10" && self.page.view == View::Lighting, place == "mouse" && self.mouse.tab == 1)
    }

    /// Where each device's lights are on Home's desk, in millimeters: the
    /// keyboard's keys with LEDs, the mouse's logo and wheel.
    fn desk_places(&self) -> crate::scenes::Places {
        let mut out = crate::scenes::Places::default();
        let item = |id: &str| self.desk.items.iter().find(|it| it.id == id).map(|it| [it.x, it.y, it.w, it.h]);
        if let (Some(r), true) = (item("z10"), self.page.dev.is_some()) {
            let vb = self.page.view_box;
            out.keys = self.page.layout.iter().filter(|k| k.led).map(|k| {
                let (x, y) = crate::scenes::on_desk(vb, r, k.cx, k.cy);
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
                let (dx, dy) = crate::scenes::on_desk(view, r, x as f64, y as f64);
                (*id, dx, dy)
            }).collect();
        }
        out
    }

    /// The scene's next frame: each light's color where it sits, sent to its
    /// device (the keyboard every frame, the mouse as often as it takes two
    /// commands, when its colors change), and the pictures with it. A device
    /// the scene leaves as it is plays its own lighting, as does one whose own
    /// Lighting page shows (see lighting_open): as the page opens, the keyboard
    /// plays the page's preview again and the mouse is handed its own lights
    /// back; as it closes, each is in again.
    fn scene_frame(&mut self) {
        use crate::scenes::Part;
        if self.scene_quit_done {
            return;
        }
        let Some(scene) = self.playing() else { return };
        let t = self.scene_t0.elapsed().as_secs_f32();
        let places = self.desk_places();
        let (kb, mouse) = (scene.lights.part("z10"), scene.lights.part("sensei310"));
        let keys: HashMap<u32, [u8; 3]> = match &kb {
            Part::Look(l) => places.keys.iter().map(|(pos, x, _)| (*pos, crate::scenes::color(l, *x, t))).collect(),
            Part::AsIs => HashMap::new(),
        };
        let lights = [0, 1].map(|i| match &mouse {
            Part::Look(l) => places.mouse.iter().find(|(id, ..)| *id == crate::mouse_art::LIGHT_IDS[i]).map(|(_, x, _)| crate::scenes::color(l, *x, t)),
            Part::AsIs => None,
        });
        self.scene_colors = (keys, lights);
        let aside = self.lighting_open();
        if aside.0 && !self.desk_aside.0 {
            let had = self.scene_claim.0;
            self.scene_claim.0 = false;
            if had {
                self.start_preview();
            }
        }
        if aside.1 != self.desk_aside.1 {
            if aside.1 && self.scene_claim.1 {
                self.mouse.hand_back();
            }
            self.scene_claim.1 = false;
            self.scene_mouse = (None, [[0; 3]; 2]);
        }
        self.desk_aside = aside;
        if self.page.dev.is_some() && !aside.0 {
            if matches!(kb, Part::Look(_)) {
                let colors: Vec<[u8; 3]> = (0..evga_z10::protocol::NKEYS as u32).map(|p| self.scene_colors.0.get(&p).copied().unwrap_or([0; 3])).collect();
                self.ask_kb(device::Ask::Preview(serde_json::json!({ "colors": colors, "tab": "native", "claim": true })));
                self.scene_claim.0 = true;
            } else if self.scene_claim.0 {
                self.ask_kb(device::Ask::Preview(serde_json::json!({ "stop": true, "tab": "native", "claim": true })));
                self.scene_claim.0 = false;
                self.last_lights = None;
            }
        }
        if self.mouse.present() && !aside.1 {
            if let (Some(logo), Some(wheel)) = (lights[0], lights[1]) {
                let due = self.scene_mouse.0.is_none_or(|at| at.elapsed() >= Duration::from_millis(125));
                // (sent again each second, unchanged: a profile put on the mouse, or the mouse
                // plugged in again, shows its own lights until it is)
                let stale = self.scene_mouse.0.is_none_or(|at| at.elapsed() >= Duration::from_secs(1));
                if due && (self.scene_mouse.1 != [logo, wheel] || stale) {
                    self.mouse.direct(serde_json::json!({ "logo": lighting::hex_of(logo), "wheel": lighting::hex_of(wheel) }));
                    self.scene_mouse = (Some(Instant::now()), [logo, wheel]);
                    self.scene_claim.1 = true;
                }
            } else if self.scene_claim.1 {
                self.mouse.hand_back();
                self.scene_claim.1 = false;
                self.scene_mouse = (None, [[0; 3]; 2]);
            }
        }
        if self.scene_form.is_some() {
            self.show_scene_strip(t);
        }
        if self.mouse_pictured() {
            self.show_mouse_lights();
        }
        // (a plugin's device met since the scene came on, OpenRGB's after its program started, gets its profile)
        if self.scene_plugins_at.elapsed() >= Duration::from_secs(1) {
            self.scene_plugins_at = Instant::now();
            self.plugins_sync();
        }
    }

    /// The scene's frame now, as its timer would give it (a snapshot has no timers).
    pub fn scene_now(&mut self) {
        self.scene_frame();
    }

    /// A mouse light's color for its picture now: the scene's, while it lights
    /// the mouse (but on the mouse's own Lighting page); else the mouse's own,
    /// none while that light is off.
    fn mouse_color(&self, i: usize, now: Instant) -> Option<[u8; 3]> {
        if !self.lighting_open().1 {
            if let Some(c) = self.scene_colors.1[i] {
                return Some(c);
            }
        }
        self.mouse.lit(crate::mouse::LIGHTS[i]).map(|_| self.mouse.color_at(i, now))
    }

    // ------------------------------------------------------------ the screen

    /// Ectomatter playing the screen, from now on.
    pub fn set_player(&mut self, p: crate::lcd_player::Player) {
        if self.lcd_paused {
            p.tell(crate::lcd_player::Control::Paused(true));
        }
        self.player = Some(p);
        self.tell_page();
        self.tell_context();
    }

    // ------------------------------------------------------------ Game launch

    /// A row of Game launch by its place: the game starting, or quitting.
    fn launch_key(i: i32) -> &'static str {
        if i <= 0 { "start" } else { "quit" }
    }

    /// What Ectomatter's settings keep for a row of Game launch (devices.z10.screen.game).
    fn game_launch_value(&self, k: &str) -> serde_json::Value {
        self.settings.get("devices.z10.screen.game").map(|v| v[k].clone()).filter(|v| v.is_object()).unwrap_or_else(|| serde_json::json!({}))
    }

    fn set_game_launch(&mut self, k: &str, field: &str, v: serde_json::Value) {
        if let Err(e) = self.settings.set(&format!("devices.z10.screen.game.{k}.{field}"), v) {
            self.status(format!("Game launch was not saved: {e}"), 2);
        }
        self.tell_game_launch();
    }

    /// Until when a row can show, as kept and as said: a game starting until its
    /// window opens or some seconds, one quitting until everything is put back or some seconds.
    fn until_choices(row: usize) -> Vec<(&'static str, &'static str)> {
        if row == 0 {
            vec![("window", "the game’s window opens"), ("5", "5 seconds have passed"), ("10", "10 seconds have passed"), ("30", "30 seconds have passed")]
        } else {
            vec![("done", "everything is put back"), ("5", "5 seconds have passed"), ("10", "10 seconds have passed")]
        }
    }

    /// What the Game launch shows as a game starts and quits, as Ectomatter's settings keep it,
    /// each row's layout from Layouts (a row is on unless turned off; before there was a Game
    /// launch, the screen's announce said whether launches broke in).
    pub fn game_launch(&self) -> crate::lcd_player::Launch {
        use crate::lcd_player::{Showing, Until};
        let was_on = self.settings.on("devices.z10.screen.announce");
        let show = |row: usize| {
            let v = self.game_launch_value(Self::launch_key(row as i32));
            let id = v["layout"].as_str().unwrap_or(crate::lcd_page::GAME_LAYOUTS[row.min(1)]);
            let until = match v["until"].as_str() {
                Some("window") => Until::Window,
                Some("done") => Until::Done,
                Some(n) if n.parse::<u32>().is_ok() => Until::Secs(n.parse().unwrap_or(5)),
                _ if row == 0 => Until::Window,
                _ => Until::Done,
            };
            Showing { on: v["on"].as_bool().unwrap_or(was_on), layout: self.screens.library.iter().find(|p| p.id == id).cloned(), until }
        };
        crate::lcd_player::Launch { start: show(0), quit: show(1) }
    }

    fn tell_game_launch(&self) {
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::GameSetup(self.game_launch()));
        }
    }

    /// Game launch's two rows, as the Screen tab shows them.
    fn show_game_rows(&mut self) {
        let launch = self.game_launch();
        let rows: Vec<crate::GlRow> = [(&launch.start, "When a game starts, show"), (&launch.quit, "When a game quits, show")]
            .iter()
            .enumerate()
            .map(|(i, (s, words))| {
                let v = self.game_launch_value(Self::launch_key(i as i32));
                let kept = v["until"].as_str().unwrap_or(if i == 0 { "window" } else { "done" }).to_string();
                let until = Self::until_choices(i).into_iter().find(|(k, _)| *k == kept).map_or("", |(_, w)| w);
                crate::GlRow {
                    on: s.on,
                    words: (*words).into(),
                    layout: s.layout.as_ref().map_or_else(|| "No layout".to_string(), |p| p.name.clone()).into(),
                    until: until.into(),
                    chosen: self.gl_pick == Some(i),
                    over: self.gl_over == Some(i),
                }
            })
            .collect();
        let ui = &self.ui;
        show_rows(ui.get_gl_rows(), |m| ui.set_gl_rows(m), rows);
    }

    /// A row's until menu, under its dropdown.
    fn open_until_menu(&mut self, row: usize, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        let kept = self.game_launch_value(Self::launch_key(row as i32))["until"].as_str().unwrap_or(if row == 0 { "window" } else { "done" }).to_string();
        let rows: Vec<MenuRow> = Self::until_choices(row).iter().map(|(k, w)| MenuRow { kind: 0, label: (*w).into(), tag: "".into(), tick: *k == kept, sub: "".into(), ..Default::default() }).collect();
        self.hang_menu(rows, under, anchor);
        self.until_menu = Some(row);
    }

    /// The game a page shows in its readings: the one starting, playing or quitting, or the
    /// last one; before any, the game played last, as it was then (for the editor's and the
    /// Screen tab's pictures).
    fn game_reading(&self) -> Option<crate::lcd_page::Game> {
        if let Some(g) = &self.game_now {
            return Some(g.clone());
        }
        let e = self.games.library.entries(&|id| self.front_apps.name(id)).into_iter().filter(|e| e.kind == crate::games::Kind::Game).max_by_key(|e| e.last)?;
        Some(crate::lcd_page::Game { name: e.name, since: e.last, ended: e.last, minutes: e.minutes, last: e.last, changing: String::new(), done: (0, 0) })
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
        self.tell_game_launch();
    }

    /// The screen plays the playlist of the profile the page shows, as it
    /// shows that profile's screen light: when another profile is chosen, in
    /// Ectomatter or on the keyboard, the screen plays that profile's playlist
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
        let outside = self.outside();
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Context(profile, outside));
        }
    }

    /// The layouts Ectomatter ships that the Library was never given, put at its end once and
    /// saved, with the list of those given, so a layout deleted after stays deleted (and a
    /// layout Ectomatter ships later is given once too).
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
            eprintln!("ectomatter: the new screen layouts were not saved: {e}");
        }
    }

    /// The Library and the playlist into Ectomatter's settings, if they
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
            let mut s = crate::lcd_stats::Stats::of(self.hosted.readings(), "page");
            s.prime();
            s
        }).read();
        r.now = Some(crate::lcd_page::Moment::now());
        r.profile = self.page.dev.as_ref().map(|_| self.page.profile_name(self.page.active()));
        r.profile_number = self.page.dev.as_ref().map(|_| self.page.active());
        r.outside = self.outside();
        r.game = self.game_reading();
        self.readings = r;
        self.read_at = Some(Instant::now());
    }

    /// The screen's light while Ectomatter plays it: the page's own, or the
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
    /// tab is open on it, in sight, and no page is edited, on a keyboard Ectomatter controls.
    /// Then the playlist stands aside, as it does for good once Ectomatter quits.
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

    /// Ectomatter closing: the screen handed back to the keyboard's own screens.
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
        let avail = crate::lcd_page::available(&self.readings, self.outside().is_some());
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
        let avail = crate::lcd_page::available(&self.readings, self.outside().is_some());
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
            (_, _) if !self.controlled => "Ectomatter leaves this keyboard to other programs.".into(),
            (State::Other, _) => "Another program is using the screen. Ectomatter plays it once that stops.".into(),
            (State::Missing, _) => "Ectomatter can’t reach the keyboard’s screen.".into(),
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
        // the turn on the screen is marked while Ectomatter plays the screen, playing or paused
        let marked = self.controlled && self.editor.is_none() && matches!(self.lcd.state, State::Playing | State::Paused);
        // the panel's head: the playlist's name in its chooser (as wide as the name needs,
        // to its most) or the field it is renamed in; the strip Space.row after it, to
        // where the round starts again (Space.row on) and then Pause (Space.inline on, at
        // the panel's padding)
        use crate::tokens::{size, space};
        let under_w = self.ui.get_under_w() as f64;
        let name = self.screens.playlist(who).name.clone();
        let chooser = (crate::form::chooser_width(&name) as f64).min(CHOOSER_MOST as f64).ceil();
        let track_x = (space::PAD + space::ROW) as f64 + if self.pl_renaming { CHOOSER_MOST as f64 } else { chooser };
        let right = (space::PAD + size::COMPACT + space::INLINE + size::ROUND + space::ROW) as f64;
        let track_w = (under_w - right - track_x).max(TRACK_LEAST);
        self.ui.set_pl_name(name.as_str().into());
        self.ui.set_pl_chooser_most(CHOOSER_MOST);
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
        // the third tab: the layout of the row selected (or a layout carried over a row's bar,
        // as it would show there; else the starting game's), drawn with the game's readings
        let game_tab = self.ui.get_screen_game() && self.editor.is_none();
        let game_page = game_tab.then(|| {
            let launch = self.game_launch();
            let carried = self.gl_over.and_then(|_| self.carry.as_ref()).and_then(|c| self.screens.library.iter().find(|p| p.id == c.page)).cloned();
            carried.or_else(|| if self.gl_pick == Some(1) { launch.quit.layout } else { launch.start.layout })
        });
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
            None if game_tab => match game_page.flatten() {
                Some(p) => crate::lcd_page::render_at(&p, &self.readings, t),
                None => vec![String::new(); 4],
            },
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
    let lines = crate::text::wrap(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, s, w);
    match lines.len() {
        0 => (String::new(), String::new()),
        1 => (lines[0].clone(), String::new()),
        2 => (lines[0].clone(), lines[1].clone()),
        _ => {
            let mut cut = lines[1..].join(" ");
            while !cut.is_empty() && crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, &format!("{}…", cut.trim_end())) > w {
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

impl Front {
    /// How much brighter than the page's colors the keys' light is drawn on
    /// the screen the window is on (see `gain_for`).
    pub fn light_gain(&self, reference: f64) -> f32 {
        gain_for(&self.settings.get("hdr").unwrap_or_default(), reference, self.screen_peak)
    }
}

/// How much brighter than the page's colors the keys' light is drawn on a
/// screen of this white and peak: the design's brightness, unless Settings ›
/// HDR lets it glow up to Peak nits, and never past the screen's own peak
/// (Peak nits is set for the brightest screen; a dimmer one stops at its own).
pub fn gain_for(hdr: &serde_json::Value, reference: f64, peak: f64) -> f32 {
    if hdr["lighting"].as_str() == Some("off") {
        return 1.0;
    }
    // up to Peak nits; at the slider's left end, the design's brightness
    hdr["glow_limit"].as_f64().map_or(1.0, |n| (n.min(peak) / reference).max(1.0) as f32)
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

    /// The keys' light glows up to Peak nits, but on a dimmer screen no
    /// higher than that screen's own peak; with the HDR interface off, or the
    /// slider at its left end, it keeps the design's brightness.
    #[test]
    fn the_light_stops_at_the_screens_own_peak() {
        let hdr = serde_json::json!({ "lighting": "auto", "glow_limit": 1000.0 });
        assert!((gain_for(&hdr, 254.0, 1000.0) - (1000.0 / 254.0) as f32).abs() < 1e-6, "the brightest screen: up to Peak nits");
        assert!((gain_for(&hdr, 73.0, 200.0) - (200.0 / 73.0) as f32).abs() < 1e-6, "a dim screen: up to its own peak");
        assert_eq!(gain_for(&serde_json::json!({ "lighting": "off", "glow_limit": 1000.0 }), 254.0, 1000.0), 1.0, "the HDR interface off");
        assert_eq!(gain_for(&serde_json::json!({ "lighting": "auto" }), 254.0, 1000.0), 1.0, "the slider at its left end");
        assert_eq!(gain_for(&serde_json::json!({ "glow_limit": 200.0 }), 254.0, 1000.0), 1.0, "a limit under the screen's white");
    }

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
        assert!(crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, &b) <= 204.0);
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
        // every layout Ectomatter ships in the Library: From scratch, then a copy of any of them
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

/// a plugin's search's rows at most, before they scroll
const PLUGIN_MENU_MOST: f32 = 320.0;

/// the playlist's chooser at its widest, and the field its playlist is renamed in; the
/// onboard layout's at its widest; and the strip after the chooser at its least
const CHOOSER_MOST: f32 = 200.0;
const OB_CHOOSER_MOST: f32 = 240.0;
const TRACK_LEAST: f64 = 120.0;

fn pic_wide(shown: f32) -> f32 {
    if shown > 0.0 { shown.round() } else { 72.0 }
}
