//! The window's pages as Ectomatter runs them (`Front`): what each page asks
//! for, what the devices and packages answer, and the models the pages are
//! drawn from. Nothing here knows about the window, so the offscreen check
//! drives it the same way. `Front` is one object; its parts are kept in the
//! files beside this one, a page or a part of one each.

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
use crate::keyboard::{self, Entry, Page, View};
use ectomatter_protocol::Action;
use crate::{AppWindow, Cell, CheckItem, DeskItem, DeskPad, EffectItem, KeyItem, LayerItem, LayerTab, LibGroup, LibRow, MenuRow, MousePatch, ProfileItem};

mod about_page;
mod action_menu;
mod color_picker;
mod first_page;
mod game_starts;
mod home_desk;
mod lighting_tab;
mod models;
mod package_browser;
mod scene_form;
mod scene_list;
mod screen_tab;
mod slider_jobs;

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
    /// in the list of packages: a row pressed, and From an address or a file…
    GlBrowsePick(i32),
    GlBrowseOther,
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
    /// a tab chosen in Settings' bar, by its place
    SettingsTab(i32),
    /// About: Build with us, Read the license, a fact's link (1 the credits, 2 the
    /// notices), a click on the page (where, and its size), Credits closed
    AboutBuild,
    AboutLicense,
    AboutLink(i32),
    AboutPoke(f32, f32, f32, f32),
    AboutCreditsClose,
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

/// What the keyboard's page says when the keyboard can't be read (`e`, the
/// core's words), by how the system shows it (`st`, devices::usb's): not
/// plugged in; plugged in but not to be opened by Ectomatter; there and not
/// answering; or, where the system can't say, the core's own words. Its
/// title, the line under it, and whether something's wrong (an unplugged
/// keyboard is only away).
fn unread(st: &serde_json::Value, e: &str) -> (String, String, bool) {
    match (st["present"].as_bool(), st["reachable"].as_bool()) {
        (Some(false), _) => ("Your keyboard isn’t plugged in".into(), "Plug it in. Ectomatter finds it within a few seconds.".into(), false),
        (Some(true), Some(false)) => ("Ectomatter can’t open your keyboard".into(), "It’s plugged in, but this computer doesn’t let Ectomatter open it. Install Ectomatter’s package, then plug the keyboard in again.".into(), true),
        (Some(true), _) => ("Your keyboard isn’t answering".into(), "It’s plugged in, but it stopped answering Ectomatter. Unplug it and plug it back in.".into(), true),
        (None, _) => ("Ectomatter can’t reach your keyboard".into(), format!("{e}. Ectomatter tries again every few seconds."), true),
    }
}

/// The page of the action menu a screen job is on.
fn screen_page(code: u32) -> usize {
    if code >= keyboard::NEXT_PLAYLIST { PLAYLIST_PAGE } else { LAYOUT_PAGE }
}

/// The Key library's groups and their keys, in the order the keyboard's
/// description lists the groups (`groups`) and the keys (each its `group`).
fn lib_groups(p: &Page) -> Vec<(String, Vec<keyboard::Key>)> {
    let mut out: Vec<(String, Vec<keyboard::Key>)> = lib_group_names().into_iter().map(|g| (g, Vec::new())).collect();
    for k in &p.layout {
        if let Some(e) = out.iter_mut().find(|(n, _)| *n == k.group) {
            e.1.push(k.clone());
        }
    }
    out
}

/// The keyboard's groups of controls, as its description orders them.
/// The Key library's groups shut as the page first opens: every one.
pub(super) fn lib_first_shut() -> std::collections::HashSet<String> {
    lib_group_names().into_iter().chain(["Sliders".to_string()]).collect()
}

fn lib_group_names() -> Vec<String> {
    crate::cores::keyboard().and_then(|k| k.device["groups"].as_array().map(|g| g.iter().filter_map(|g| g.as_str().map(String::from)).collect())).unwrap_or_default()
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
    /// the playlist each code a key may play one with plays (see keyboard::PLAY_CODES), by its
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
    slider_ends: Vec<[f64; 2]>,
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
    /// Add a core or Add a plugin, from the list Ectonomic reviewed; and a rule for a core's
    /// devices being written, with the system's password prompt (the core's name)
    browser: Option<package_browser::Browser>,
    granting: Option<(String, mpsc::Receiver<Result<(), String>>)>,
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
    /// Settings' About: a Read being opened (0 the license, 1 the notices), and what
    /// went wrong with the last
    about_read: Option<(i32, mpsc::Receiver<Result<(), String>>)>,
    about_said: Option<(i32, String)>,
    /// About's own: its credits, its Blob, its Credits dialog (about_page.rs)
    about: about_page::AboutState,
    /// Settings' tab open: General as Ectomatter starts (ECTOMATTER_SETTINGS_TAB, a snapshot's)
    pub settings_tab: crate::ecto_settings::Tab,
    /// what starts at login, as the system keeps it (autostart.rs), and when it was looked at
    starts: (crate::autostart::Starts, Instant),
    /// what Settings' bar says, whether it's a problem, and since when
    settings_said: Option<(String, bool, Instant)>,
    /// an added plugin asked about removing
    removing: Option<String>,
    mouse_icons: Option<(f32, f32, Vec<(&'static str, slint::Image)>)>,
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
    /// the place a press came down on, and whether it stood at its first page before the
    /// press let anything chosen on it go: what a rail item's click (on the release) goes by
    press_first: Option<(String, bool)>,
    pick: Option<Pick>,
    overlay_model: Rc<VecModel<crate::Overlay>>,
    /// what each slider controls, which slider's menu is open, and what the
    /// sliders last read and last set
    bind: Vec<String>,
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
    pub macro_jobs: Option<crate::shell::Sender<String>>,
    /// whether the keyboard's core says the keyboard is plugged in (kept by the
    /// keyboard's thread), and whether it did when last looked at
    pub kb_there: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    kb_was_there: bool,
    /// a slider set to Nothing because the other took its job: the job it had
    idled: Vec<Option<String>>,
    last_sliders: Option<serde_json::Value>,
    held: Option<u8>,
    sent: Vec<Option<f64>>,
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
    /// The pages over the packages running (`hosted`, every one that's on
    /// started), the keyboard's thread (`kb`) and whether Ectomatter controls
    /// the keyboard.
    pub fn new(ui: AppWindow, hosted: crate::hosted::Hosted, kb: Option<mpsc::Sender<device::Ask>>, controlled: bool, settings: crate::settings::Settings) -> Front {
        // (which sensors the screen's layouts can show, as last chosen)
        let setup = crate::sensors::Setup::of(&settings.all()["sensors"]);
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
        on!(on_settings_tab_chosen, |i| Ask::SettingsTab(i));
        on!(on_about_build, Ask::AboutBuild);
        on!(on_about_license, Ask::AboutLicense);
        on!(on_about_fact_link, |i| Ask::AboutLink(i));
        on!(on_about_poke, |x, y, w, h| Ask::AboutPoke(x, y, w, h));
        on!(on_about_credits_close, Ask::AboutCreditsClose);
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
        on!(on_gl_browse_pick, |i| Ask::GlBrowsePick(i));
        on!(on_gl_browse_other, Ask::GlBrowseOther);
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
        // (each of the keyboard's sliders, as its core describes them: the ends seen of each one's
        // readings, from the range its core gives; what each controls; one rested from its job)
        let sliders = crate::display::sliders();
        let mut slider_ends: Vec<[f64; 2]> = crate::uistate::get("slider_ends").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        slider_ends.extend(sliders.iter().skip(slider_ends.len()).map(|s| s.range));
        let mut bind: Vec<String> = crate::uistate::get("slider_bind").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        bind.resize(sliders.len(), "own".into());
        let mut idled: Vec<Option<String>> = crate::uistate::get("slider_idled").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        idled.resize(sliders.len(), None);
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
            browser: None,
            granting: None,
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
            about_read: None,
            about_said: None,
            about: Default::default(),
            // (the tab last left; a snapshot's from ECTOMATTER_SETTINGS_TAB)
            settings_tab: std::env::var("ECTOMATTER_SETTINGS_TAB").ok().and_then(|n| crate::ecto_settings::Tab::named(&n)).unwrap_or_default(),
            starts: (crate::autostart::now(), Instant::now()),
            settings_said: None,
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
            press_first: None,
            ui,
            page,
            kb,
            asks,
            menu: None,
            layer: 0,
            // (every group shut at first)
            lib_shut: lib_first_shut(),
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
            bind,
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
            kb_there: None,
            kb_was_there: true,
            idled,
            last_sliders: None,
            held: None,
            sent: vec![None; sliders.len()],
            released: None,
            settings,
            typing: None,
            typed_at: None,
            found: (Vec::new(), String::new()),
            found_tx: ftx,
            found_rx: frx,
            search_menu: false,
            plugin_map: Default::default(),
            // (until the screens are read: SDR's white as BT.2408 has it, as
            // main.rs `levels` takes for a screen that doesn't say)
            levels: (203.0, 1000.0),
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
            f.settings.get(&crate::cores::keyboard_setting("screen.pages")).as_ref(),
            f.settings.get(&crate::cores::keyboard_setting("screen.playlists")).as_ref(),
            f.settings.get(&crate::cores::keyboard_setting("screen.plays")).as_ref(),
            f.settings.get(&crate::cores::keyboard_setting("screen.playlist")).as_ref(),
        )
        .with_onboard(f.settings.get(&crate::cores::keyboard_setting("screen.onboard")).as_ref());
        f.give_built_ins();
        f.keyed = f
            .settings
            .get(&crate::cores::keyboard_setting("screen.keyed"))
            .and_then(|v| v.as_object().cloned())
            .map(|m| m.iter().filter_map(|(c, id)| Some((c.parse::<u32>().ok().filter(|c| keyboard::PLAY_CODES.contains(c))?, id.as_str()?.to_string()))).collect())
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
            let _ = f.settings.set(&crate::cores::keyboard_setting("screen.keyed"), serde_json::Value::Object(kept));
        }
        crate::jobs::rename_in_macros();
        f.screens_noted = f.screens.clone();
        f.screens_saved = f.screens.clone();
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
        // (who makes the keyboard, as its core says, for the page's words about its own screens)
        f.ui.set_kb_maker(crate::cores::keyboard().and_then(|k| k.described()["vendor"].as_str()).unwrap_or_default().into());
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

    /// What was just done on Settings, or what went wrong (`bad`), in its bar: words that
    /// go after a while, or a problem that stays until the next.
    pub(super) fn settings_say(&mut self, text: impl Into<String>, bad: bool) {
        let text = text.into();
        self.ui.set_settings_said(text.as_str().into());
        self.ui.set_settings_said_bad(bad);
        self.settings_said = Some((text, bad, Instant::now()));
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
                if let Some(k) = crate::cores::keyboard() {
                    k.describe(&d);
                }
                self.page.desc = d;
                self.icons = None;
            }
            device::Heard::State(Ok(dev)) => {
                self.failed_at = None;
                self.page.loaded(dev);
                // the keyboard already shows the screen of the profile it is using
                let n = self.page.active();
                self.screen_sent = self.page.dev.as_ref().map(|d| screen_of(&d["profiles"][n.to_string()]["lighting"]));
                self.tell_free();
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
                self.say_unread(&e);
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
            device::Heard::Stopped(why, again) => {
                self.status(format!("The keyboard’s core stopped: {why}. Ectomatter starts it again {}.", crate::core_host::again_in(again)), 2);
                return;
            }
            device::Heard::Restarted => {
                self.status("The keyboard’s core started again.", 1);
                self.ask_kb(device::Ask::State);
                return;
            }
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
                    if crate::cores::profiles().contains(&(n as u8)) && n != self.page.active() {
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

    /// The keyboard couldn't be read (`e`, its core's words), or is gone since it
    /// was: its page says why, as the system shows the keyboard (not plugged in,
    /// not to be opened, or there and not answering), in place of what's on it,
    /// till it's read again.
    fn say_unread(&mut self, e: &str) {
        let usb = crate::cores::keyboard().and_then(|k| self.packages.iter().find(|p| p.id() == k.id)).and_then(|p| p.manifest.usb_ids());
        let st = usb.map(|(v, p)| crate::devices::usb(v, p)).unwrap_or_default();
        let (title, text, bad) = unread(&st, e);
        self.ui.set_conn_state(2);
        self.ui.set_conn_text(title.as_str().into());
        // (2 says it in the color of what's wrong; 3 in the page's own)
        self.ui.set_boot(if bad { 2 } else { 3 });
        self.ui.set_boot_title(title.into());
        self.ui.set_boot_text(text.into());
    }

    /// Try the keyboard again a few seconds after it could not be read.
    pub fn retry(&mut self) {
        // (one read before and unplugged since says so; plugged in again, it's read again)
        if let Some(there) = self.kb_there.as_ref().map(|t| t.load(std::sync::atomic::Ordering::Relaxed)) {
            if there != self.kb_was_there {
                self.kb_was_there = there;
                if !there && self.controlled && self.page.dev.is_some() {
                    self.say_unread("the keyboard isn’t connected");
                } else if there && self.controlled {
                    self.ask_kb(device::Ask::State);
                }
            }
        }
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
                        if self.scene_lighting(crate::cores::keyboard_key()).is_some() {
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
                Ask::SettingsTab(i) => {
                    self.close_menu();
                    self.close_picker();
                    self.open_settings_tab(crate::ecto_settings::Tab::at(i));
                }
                // (a Read opened away from the window's thread: about_tick says what went wrong)
                Ask::AboutBuild | Ask::AboutLicense | Ask::AboutLink(2) => {
                    let which = match ask { Ask::AboutBuild => 2, Ask::AboutLicense => 0, _ => 1 };
                    self.about_said = None;
                    self.about_read = Some((which, crate::about::read(which)));
                }
                Ask::AboutLink(_) => self.open_credits(),
                Ask::AboutPoke(x, y, w, h) => self.poke_blob(x, y, w, h),
                Ask::AboutCreditsClose => self.close_credits(),
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
                    for b in 0..crate::mouse::shape().buttons.len() {
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
                        if self.scene_lighting(crate::mouse::key()).is_some() {
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
                        // (stopped once nothing moves: a press's flash faded)
                        self.mouse_frames_run();
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
                    // (a plugin started again after it stopped, handed its values again)
                    for id in self.hosted.take_restarted() {
                        self.hand_values(Some(&id));
                    }
                    let met = self.hosted.take_added();
                    if !met.is_empty() {
                        for k in &met {
                            self.plugin_applied.remove(k);
                        }
                        self.plugins_sync();
                    }
                    news |= self.worker_told();
                    news |= self.launch_tick();
                    news |= self.about_tick();
                    if self.credits_came() && self.about_shows() {
                        self.refresh();
                    }
                    news |= self.games.typed_settled();
                    if self.package_form.is_some() {
                        self.package_form_tick();
                    }
                    if self.browser.is_some() {
                        self.browser_tick();
                    }
                    self.granting_tick();
                    while let Ok(id) = self.updates.updated.try_recv() {
                        // an update put in place: its plugin started again on it
                        self.give_packages();
                        if let Some(p) = self.packages.iter().find(|p| p.id() == id).cloned() {
                            // (a core runs the new version from Ectomatter's next start, with its device's page)
                            if self.hosted.get(&id).is_some() && p.kind() == crate::packages::Kind::Plugin {
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
                    let on_screen = self.page.view == View::Screen && self.ui.get_place() == "keyboard";
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
                    let on = crate::display::game_keys().get(i as usize).is_some_and(|g| blocked & g.0 != 0);
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
                            let screen = self.page.sel.map(|pos| self.page.def(LAYERS[self.layer].0, pos)).filter(|d| !self.is_job_key(d)).and_then(|d| keyboard::screen_job(&d)).map_or(LAYOUT_PAGE, screen_page);
                            let page = [0, 1, 2, MACRO_PAGE, 3, screen, 6][i.clamp(0, 6) as usize];
                            self.open_menu_on(self.layer, (x, y, h), Some(page), 0.0);
                        }
                    }
                }
                Ask::SliderSel(i) => {
                    self.slider_sel = i.clamp(0, crate::display::sliders().len() as i32) as usize;
                    if self.slider_sel != 0 {
                        self.page.sel = None;
                        self.listening = None;
                        self.close_menu();
                    }
                }
                Ask::SliderBind(i) => {
                    if self.slider_sel != 0 {
                        let s = self.slider_sel - 1;
                        if let Some((v, _)) = self.slider_choices(s).get(i as usize).cloned() {
                            self.set_slider_bind(s, &v);
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
                    if self.settings_said.as_ref().is_some_and(|(_, bad, t)| !bad && t.elapsed() > Duration::from_secs(6)) {
                        self.settings_said = None;
                        self.ui.set_settings_said("".into());
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
                    if self.browser.is_some() {
                        self.browse_add_done();
                    } else if self.package_form.is_some() {
                        self.package_add_done();
                    } else {
                        self.games.add_done(&self.ui);
                    }
                }
                Ask::GlClose => {
                    if self.browser.take().is_some() || self.package_form.take().is_some() {
                        self.ui.set_gl_modal(0);
                    } else {
                        self.games.close_form();
                    }
                }
                Ask::GlBrowsePick(i) => self.browse_pick(i),
                Ask::GlBrowseOther => self.browse_other(),
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
                // an item opens its place at its first page (first_page.rs); the item already
                // chosen, its place at its first page already, opens the rail into its blade
                // or closes it. The lab is a window of its own, not a place in this one
                Ask::Rail(place) if place == "lab" => {
                    self.close_menu();
                    window.push(WindowAsk::OpenLab);
                }
                Ask::Rail(place) => {
                    self.close_menu();
                    let first = match self.press_first.take() {
                        Some((p, first)) if p == place => first,
                        _ => self.at_first_page(&place),
                    };
                    if self.ui.get_place() == place.as_str() && first {
                        let open = !self.ui.get_rail_open();
                        self.ui.set_rail_open(open);
                        crate::uistate::set("rail_open", serde_json::json!(open));
                    } else {
                        self.to_first_page(&place);
                        self.ui.set_place(place.into());
                    }
                }
                Ask::PollSliders => {
                    changed = false;
                    if self.page.reads_sliders() && self.wants_slider_readings() && !self.sliders_asked && self.kb.is_some() {
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
        crate::keyboard::set_bleeds(light);
        let controls = crate::cores::keyboard().and_then(|c| c.device["controls"].as_array().cloned()).unwrap_or_default();
        self.pale = controls.iter().filter_map(|c| Some((c["id"].as_str()?, c["pos"].as_u64()? as u32))).filter(|(id, _)| ectomatter_draw::hdr::pale_top(light, id)).map(|(_, pos)| pos).collect();
    }

    /// Do what the lab asked; what only its window can do is handed back.
    /// (The picker's answer for a typeface, not a part's picture.)
    pub fn act_lab(&mut self) -> Vec<WindowAsk> {
        let Some(l) = &mut self.lab else { return Vec::new() };
        let window = l.act();
        // the light setup chosen to light the keyboard, kept for next time
        if let Some(setup) = l.switched.take() {
            let _ = self.settings.set(&crate::setups::in_use_at(), setup.map_or(serde_json::Value::Null, serde_json::Value::from));
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
                        (Some(path), _) => match crate::setups::import_image(l.core(), &path) {
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

    /// The display in the picture of the keyboard while Ectomatter's own player isn't
    /// driving the screen: unlit, as nothing Ectomatter knows of is on it. (While the player
    /// drives it, the picture follows each frame as the player sends it: see
    /// [Front::screen_news].)
    fn poll_lcd(&mut self) {
        if !self.player_drives() {
            self.show_lcd(false, &[]);
        }
    }

    /// Whether Ectomatter's own player is what the keyboard's screen shows now.
    fn player_drives(&self) -> bool {
        self.controlled && matches!(self.lcd.state, crate::lcd_player::State::Playing | crate::lcd_player::State::Paused)
    }

    /// The display in the picture of the keyboard showing `rows` (unlit unless `live`),
    /// in the screen's light, drawn again only when that changes.
    fn show_lcd(&mut self, live: bool, rows: &[String]) {
        let (cols, lines) = crate::lcd_page::size();
        let rows: Vec<String> = rows.iter().take(lines).map(|r| format!("{:<cols$}", r.chars().take(cols).collect::<String>())).collect();
        let lcd = (live && !rows.is_empty(), rows, self.page.lcd_color());
        if lcd != self.overlays.lcd {
            self.overlays.lcd = lcd;
            self.show_overlays();
        }
    }

    /// Where the sliders are: each reading becomes a knob's place between the
    /// lowest and highest readings seen for its slider.
    fn move_knobs(&mut self, v: &serde_json::Value) {
        let sliders = crate::display::sliders();
        let raws: Vec<Option<f64>> = sliders.iter().map(|s| v[format!("{}_raw", s.id)].as_f64()).collect();
        let mut wider = false;
        self.overlays.fracs.resize(sliders.len(), None);
        self.slider_ends.extend(sliders.iter().skip(self.slider_ends.len()).map(|s| s.range));
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
        let fr: Vec<f32> = self.overlays.fracs.iter().map(|f| f.unwrap_or(0.0)).collect();
        // the chosen slider's box follows it as it moves
        if self.slider_sel != 0 {
            self.ui.set_slider_at(fr.get(self.slider_sel - 1).copied().unwrap_or(0.0));
        }
        self.drive_sliders(v, &fr);
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

    /// A keyboard that can't be read is said to be unplugged, not to be opened,
    /// or not answering, as the system shows it; the core's words where it can't.
    #[test]
    fn an_unread_keyboard_says_why() {
        let said = |present: serde_json::Value, reachable: serde_json::Value| unread(&serde_json::json!({ "present": present, "reachable": reachable }), "no reply").0;
        assert_eq!(said(false.into(), serde_json::Value::Null), "Your keyboard isn’t plugged in");
        assert_eq!(said(true.into(), false.into()), "Ectomatter can’t open your keyboard");
        assert_eq!(said(true.into(), true.into()), "Your keyboard isn’t answering");
        let (title, text, bad) = unread(&serde_json::Value::Null, "no reply");
        assert_eq!((title.as_str(), text.as_str(), bad), ("Ectomatter can’t reach your keyboard", "no reply. Ectomatter tries again every few seconds.", true));
        assert!(!unread(&serde_json::json!({ "present": false }), "").2, "an unplugged keyboard is only away");
    }

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

/// the Weather Feed plugin's id, as its manifest names it (the screens' weather is its
/// readings)

/// the playlist's chooser at its widest, and the field its playlist is renamed in; the
/// onboard layout's at its widest; and the strip after the chooser at its least
const CHOOSER_MOST: f32 = 200.0;
const OB_CHOOSER_MOST: f32 = 240.0;
const TRACK_LEAST: f64 = 120.0;

fn pic_wide(shown: f32) -> f32 {
    if shown > 0.0 { shown.round() } else { 72.0 }
}
