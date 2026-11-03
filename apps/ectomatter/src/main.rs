//! Ectomatter as a native window: every pixel drawn by Ectomatter, no web view.
//!
//! The window is one HDR picture. The page (ui/app.slint) is drawn by Slint's
//! software renderer. The keyboard is drawn from the Z10 core's art by
//! ectomatter-draw and lit in linear light, brighter than the desktop's white
//! where it glows. The page leaves a hole where the keyboard goes; the two
//! meet in one picture, in linear light, which goes to the compositor as PQ
//! with sRGB's primaries (BT.709), 16 bits a channel. The page's colors are
//! decoded as KWin decodes every SDR app's (gamma 2.2 at the reference
//! white), so the page looks as the web app did and only the light goes
//! above it.
//!
//! `ectomatter --snapshot FILE` draws one frame at 1440 x 940 without a
//! window and saves the picture as raw ABGR16161616, for checking. It reads
//! the keyboard's state from ECTOMATTER_STATE (a file the window saves
//! into ECTOMATTER_DUMP when it reads the keyboard), and that a read after
//! it, or the only one, failed with ECTOMATTER_UNREAD's words (the core's),
//! clicks where
//! ECTOMATTER_CLICKS says ("x,y;x,y", logical pixels) and types
//! ECTOMATTER_TYPE, so each state of the page can be checked offscreen.
//!
//! `ectomatter --hide` starts into the tray, as it starts at login.

// (on systems other than Linux, what only the Wayland shell uses goes unused)
#![cfg_attr(not(target_os = "linux"), allow(dead_code, unused_imports, unused_mut))]

// A function that can only call itself is a crash waiting for its caller, so
// it stops the build rather than warning (one slipped through in 1da0b0b).
#![deny(unconditional_recursion)]

mod about;
mod blob;
mod motion;
mod autostart;
mod adapter;
mod boxes;
#[cfg(target_os = "linux")]
mod bus;
mod boundaries;
mod clipboard;
mod clock;
mod commands;
mod computer;
mod core_host;
mod cores;
mod described;
mod desk;
mod device;
mod display;
mod devices;
mod ecto_settings;
mod files;
mod form;
mod front;
mod front_app;
mod games;
mod games_page;
mod hosted;
mod ink_env;
mod jobs;
mod keyboard;
mod menu_size;
#[cfg(target_os = "linux")]
mod kde_output;
mod launcher;
#[cfg(target_os = "linux")]
mod shadow;
mod shell;
mod sound;
mod lab;
mod layout_page;
mod lcd_anim;
mod lcd_draw;
mod lcd_fx;
mod lcd_page;
#[cfg(test)]
mod lcd_parity;
mod lcd_player;
mod lcd_stats;
mod library;
mod lighting;
mod locks;
mod log;
mod macro_page;
mod macros;
mod monitors;
mod mouse;
mod mouse_art;
mod nvml;
mod old_name;
mod once;
mod overlays;
mod packages;
mod picker;
mod picture;
mod profiles;
mod permission;
mod putback;
mod registry;
mod recording;
mod screen_edit;
mod screen_keys;
mod scenes;
mod tokens;
mod screens;
mod sensors;
mod settings;
mod setups;
mod steam;
mod text;
mod typefaces;
mod tray;
mod typist;
mod updates;
mod uistate;
mod vdf;

slint::include_modules!();

use std::cell::RefCell;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ectomatter_draw::hdr::sim::{self, Scene, Setup, Simulator, Snapshot};
use ectomatter_draw::hdr::{Board, Light};
use front::{Front, WindowAsk};
use picture::{Lit, Picture};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use adapter::SoftWindow as MinimalSoftwareWindow;
use slint::platform::software_renderer::{PremultipliedRgbaColor, RepaintBufferType};
use slint::platform::{Key, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{LogicalPosition, PhysicalSize, SharedString};

/// the size the window opens at: under a 1080p screen with a taskbar, and
/// tall enough that the Keys and Settings tabs show whole under the keyboard,
/// which scales with the window's width; and the least it may be
const SIZE: (u32, u32) = (1440, 1028);
const MIN_SIZE: (u32, u32) = (1000, 700);
/// the Macro Editor's window, as the web app's
const MACRO_SIZE: (u32, u32) = (1080, 620);
const MACRO_MIN_SIZE: (u32, u32) = (900, 420);
/// the lab's
const LAB_SIZE: (u32, u32) = (1280, 800);
const LAB_MIN_SIZE: (u32, u32) = (1080, 680);
/// the screen layout editor's, as the main window's (its screen at the size the Screen tab's is)
const LAYOUT_SIZE: (u32, u32) = (1440, 1028);
const LAYOUT_MIN_SIZE: (u32, u32) = (1200, 760);
/// the windows of their own beside the main one, by their place in App's `subs`
const MACROS: usize = 0;
const LAB: usize = 1;
const LAYOUTS: usize = 2;
/// the ground the keyboard sits on: its stage's, every device's (Ink.stage)
const STAGE: [u8; 3] = tokens::ink::STAGE;
/// the ground it sits on on Home's desk: the app's own (Ink.base)
const HOME_GROUND: [u8; 3] = tokens::ink::BASE;
/// How much brighter than the page's colors the keys' light is drawn. The
/// light is drawn at the web design's brightness, as approved in WebKit; going
/// above the desktop's white by the screen's headroom looked overdone.
const LIGHT_GAIN: f32 = 1.0;



/// Slint, told to draw into our own buffers and to keep time by our clock.
/// Each window it is asked for is kept, in order, for the window to draw.
struct Platform {
    made: Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>,
    start: Instant,
}

impl slint::platform::Platform for Platform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let w = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        self.made.borrow_mut().push(w.clone());
        Ok(w)
    }
    fn duration_since_start(&self) -> Duration {
        clock::animations(self.start)
    }
    // the desktop's clipboard, through Wayland's (clipboard.rs); none of the selection's
    fn set_clipboard_text(&self, text: &str, clipboard: slint::platform::Clipboard) {
        if clipboard == slint::platform::Clipboard::DefaultClipboard {
            clipboard::copied(text);
        }
    }
    fn clipboard_text(&self, clipboard: slint::platform::Clipboard) -> Option<String> {
        (clipboard == slint::platform::Clipboard::DefaultClipboard).then(clipboard::text).flatten()
    }
}

/// A glyph from the app's drawings, as white on clear, for the page to tint.
/// A package's glyph drawn at a size, in white for the window to tint (a
/// glyph that can't be read is left empty).
pub fn package_glyph(file: &std::path::Path, px: u32) -> slint::Image {
    let Ok(svg) = std::fs::read_to_string(file) else { return slint::Image::default() };
    let svg = svg.replace("currentColor", "#ffffff");
    let Ok(tree) = usvg::Tree::from_str(&svg, &usvg::Options::default()) else { return slint::Image::default() };
    let Some(mut pix) = Pixmap::new(px, px) else { return slint::Image::default() };
    let s = tree.size();
    resvg::render(&tree, Transform::from_scale(px as f32 / s.width(), px as f32 / s.height()), &mut pix.as_mut());
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(px, px);
    buf.make_mut_bytes().copy_from_slice(pix.data());
    slint::Image::from_rgba8_premultiplied(buf)
}

/// A plus, in a menu's gray, `px` pixels square: New scene…'s, where a scene's chip goes.
pub fn plus_glyph(px: u32) -> slint::Image {
    glyph(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none" stroke="#a1a1aa" stroke-width="1.6" stroke-linecap="round"><path d="M7 2.5v9M2.5 7h9"/></svg>"##, px.max(14), false)
}

fn glyph(svg: &str, px: u32, tint: bool) -> slint::Image {
    let svg = if tint { svg.replace("currentColor", "#ffffff") } else { svg.to_string() };
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("a glyph the app drew");
    let mut pix = Pixmap::new(px, px).unwrap();
    let s = tree.size();
    resvg::render(&tree, Transform::from_scale(px as f32 / s.width(), px as f32 / s.height()), &mut pix.as_mut());
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(px, px);
    buf.make_mut_bytes().copy_from_slice(pix.data());
    slint::Image::from_rgba8_premultiplied(buf)
}

/// The computer's locale, such as "en-US", for the defaults it implies.
pub fn locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en-US".into())
}

/// Ectomatter's own folders, the web app's, so the two share settings and backups.
fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")).join("com.ectonomic.ectomatter")
}
fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")).join("com.ectonomic.ectomatter")
}
/// and what it can make again whenever it's asked for (the notices, run from the source folder)
fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")).join("com.ectonomic.ectomatter")
}

/// How bright the desktop's white is and how bright the keys' light can go
/// on the brightest screen: up to Peak nits, and no higher than its peak.
fn levels(settings: &settings::Settings) -> (f64, f64) {
    let screens = screens::screens();
    let best = screens.iter().max_by(|a, b| a["peak"].as_f64().partial_cmp(&b["peak"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    // (the white the screen's user set, which an idle dim does not change)
    let reference = best.and_then(|s| s["white"].as_f64().or(s["reference"].as_f64())).filter(|r| *r > 0.0).unwrap_or(203.0);
    let peak = best.and_then(|s| s["peak"].as_f64()).unwrap_or(reference).max(reference);
    (reference, front::gain_for(&settings.get("hdr").unwrap_or_default(), reference, peak) as f64 * reference)
}


/// Build the keyboard at each size asked for, away from the window's thread:
/// it takes a third of a second, and the window keeps drawing meanwhile. Its
/// legends are set as the light in use asks before it is handed over, so a
/// keyboard drawn anew never shows the drawing's own legends first.
///
/// One builds the keyboard for its page, on its box's ground; another for
/// Home's desk, on the app's ground (`home`), each telling which it built.
fn board_builder(done: shell::Sender<(bool, Board)>, home: bool) -> std::sync::mpsc::Sender<(u32, u32, serde_json::Value, f32)> {
    let (ask, asked) = std::sync::mpsc::channel::<(u32, u32, serde_json::Value, f32)>();
    std::thread::spawn(move || {
        // (with no keyboard, nothing to build: every ask is let go)
        let Some(core) = cores::keyboard() else { return asked.into_iter().for_each(drop) };
        let (tree, device) = (core.tree(), core.device_now());
        while let Ok(mut size) = asked.recv() {
            // only the latest size matters
            while let Ok(s) = asked.try_recv() {
                size = s;
            }
            let t = Instant::now();
            match Board::new(&tree, &device, if home { ink_env::rgb("base", HOME_GROUND) } else { ink_env::rgb("stage", STAGE) }, size.0, size.1) {
                Ok(mut b) => {
                    b.set_legends_from(&size.2);
                    b.lay_shadow(&stage_shadow(size.3), STAGE_SHADOW_OPACITY);
                    eprintln!("ectomatter: keyboard drawn at {}x{} in {:?}{}", size.0, size.1, t.elapsed(), if home { ", for Home" } else { "" });
                    if done.send((home, b)).is_err() {
                        return;
                    }
                }
                Err(e) => eprintln!("ectomatter: the keyboard could not be drawn: {e}"),
            }
        }
    });
    ask
}

/// How dark the device's shadow on its stage is drawn, as a whole: half its
/// layers' own (Grant's 50%, 2026-10-20)
const STAGE_SHADOW_OPACITY: f32 = 0.5;

/// The shadow a device casts on its stage, for a window `scale` pixels to the
/// page's: two soft layers, a wide one and a close one (as approved in the
/// mouse's mock, mock/sensei-310-contrast.html: CSS drop shadows of 0 8px
/// 22px at 60% black and 0 2px 7px at 35%), the whole drawn at
/// STAGE_SHADOW_OPACITY.
fn stage_shadow(scale: f32) -> [ectomatter_draw::hdr::ShadowLayer; 2] {
    use ectomatter_draw::hdr::ShadowLayer;
    [
        ShadowLayer { down: 8.0 * scale, blur: 22.0 * scale, strength: 0.6 },
        ShadowLayer { down: 2.0 * scale, blur: 7.0 * scale, strength: 0.35 },
    ]
}

/// The part of a device's shadow round its picture, as the page draws it:
/// black, as dark as the shadow, and clear over the picture itself (which
/// has that part of the shadow in it).
fn shadow_image(s: &ectomatter_draw::hdr::Shadow, w: u32, h: u32) -> slint::Image {
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(s.w, s.h);
    let m = s.margin;
    for (j, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (j as u32 % s.w, j as u32 / s.w);
        let over = x >= m && x < m + w && y >= m && y < m + h;
        let a = if over { 0 } else { (s.dark[j] * 255.0).round().clamp(0.0, 255.0) as u8 };
        // premultiplied: black is all zeros but its alpha
        *px = slint::Rgba8Pixel { r: 0, g: 0, b: 0, a };
    }
    slint::Image::from_rgba8_premultiplied(buf)
}

/// Set the legends a light asks for, away from the window's thread: setting
/// them takes a moment, and only the latest asked for each keyboard matters
/// (the page's and Home's are asked for one after the other).
fn legends_builder(done: shell::Sender<ectomatter_draw::hdr::Legends>) -> std::sync::mpsc::Sender<ectomatter_draw::hdr::LegendsAsk> {
    let (ask, asked) = std::sync::mpsc::channel::<ectomatter_draw::hdr::LegendsAsk>();
    std::thread::spawn(move || {
        while let Ok(a) = asked.recv() {
            let mut todo = vec![a];
            while let Ok(later) = asked.try_recv() {
                todo.retain(|a| !a.for_same(&later));
                todo.push(later);
            }
            for a in todo {
                let t = Instant::now();
                let legends = a.set(&ectomatter_draw::print::faces());
                eprintln!("ectomatter: legends set in {:?}", t.elapsed());
                if done.send(legends).is_err() {
                    return;
                }
            }
        }
    });
    ask
}

/// A brightness for the lab's readout: whole nits, or tenths below ten.
fn tenth(nits: f32) -> f64 {
    let n = nits as f64;
    if n < 10.0 { (n * 10.0).round() / 10.0 } else { n.round() }
}

/// The keyboard drawn at a size and lit by its light simulated to the end,
/// here and now: for a snapshot, which has no window to wait in.
fn settled(tree: &usvg::Tree, device: &serde_json::Value, ground: [u8; 3], w: u32, h: u32) -> Board {
    let mut board = Board::new(tree, device, ground, w, h).unwrap();
    let snap = Simulator::settle(&Scene::of(&board), &board.setup(&device["light"]), sim::SETTLED);
    board.set_sim(std::sync::Arc::new(snap));
    board
}

/// The core's device description, its light as the setup in use makes it
/// (the core's own, with none of the person's own in use).
fn device_in_use(core: &cores::Core, settings: &settings::Settings) -> serde_json::Value {
    let mut device = core.device_now();
    // (ECTOMATTER_LIGHT_SETUP=id: a snapshot of the keyboard lit by that setup of
    // the person's own, as if chosen, without changing their choice)
    let chosen = std::env::var("ECTOMATTER_LIGHT_SETUP").ok().and_then(|id| setups::read(&core.id, &id));
    if let Some(s) = chosen.or_else(|| setups::in_use(settings, &core.id)) {
        device["light"] = setups::merged(&device["light"], &s.changes);
    }
    device
}

/// The devices' records given what their cores, started, describe, which
/// their pages are drawn from: the keyboard's profiles, lights and screen,
/// and the first mouse's buttons, lights and picture.
fn devices_described(hosted: &hosted::Hosted) {
    // (a core that's off is started alone for a moment to say, so its
    // device's settings, jobs and Library choices are still found by its id)
    let wait = std::time::Duration::from_secs(10);
    let off: Vec<packages::Package> = packages::of(packages::Kind::Core).into_iter().filter(|p| hosted.get(p.id()).is_none()).collect();
    if let Some(k) = cores::keyboard() {
        match hosted.get(&k.id) {
            Some(r) => k.describe(&r.describes),
            None => {
                if let Some(p) = off.iter().find(|p| p.id() == k.id) {
                    k.describe(&core_host::described_alone(&p.dir, wait));
                }
            }
        }
    }
    if let Some(r) = hosted.cores.iter().find(|r| r.describes["kind"] == "mouse") {
        mouse::describe(r.package.id(), &r.package.dir, &r.describes);
    } else {
        for p in off.iter().filter(|p| cores::keyboard().is_none_or(|k| k.id != p.id())) {
            let d = core_host::described_alone(&p.dir, wait);
            if d["kind"] == "mouse" {
                mouse::describe(p.id(), &p.dir, &d);
                break;
            }
        }
    }
}



/// The typefaces the person's setups and the core's light name, all keys'
/// and keys' own.
fn setups_typefaces(core: &cores::Core) -> Vec<String> {
    let mut lights: Vec<serde_json::Value> = setups::list(&core.id).into_iter().map(|s| s.changes).collect();
    lights.push(core.device_now()["light"].clone());
    let mut out: Vec<String> = Vec::new();
    for l in &lights {
        let keys = l["keys"].as_object().map(|k| k.values().cloned().collect::<Vec<_>>()).unwrap_or_default();
        for v in std::iter::once(&l["legends"]["typeface"]).chain(keys.iter().map(|k| &k["legends"]["typeface"])) {
            if let Some(t) = v.as_str().filter(|t| !out.iter().any(|o| o == t)) {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// Ectomatter's own fonts for a keyboard's glyphs in the windows' text too
/// (the lab's glyph list, a key's text as typed): looked in after the
/// computer's own fonts, so a glyph the computer has no font for still shows.
fn glyph_fonts_in_windows() {
    use i_slint_core::textlayout::sharedparley::fontique::{Blob, GenericFamily};
    let _ = i_slint_core::context::with_global_context(
        || Err(slint::PlatformError::NoPlatform),
        |ctx| {
            let mut fonts = ctx.font_context().borrow_mut();
            let ours: Vec<_> = ectomatter_draw::print::GLYPH_FONTS.iter().flat_map(|&data| fonts.collection.register_fonts(Blob::new(std::sync::Arc::new(data)), None)).map(|(family, _)| family).collect();
            // the last of the families Slint looks in for a glyph, after the computer's own
            let computer: Vec<_> = fonts.collection.generic_families(GenericFamily::SystemUi).collect();
            fonts.collection.set_generic_families(GenericFamily::SystemUi, computer.into_iter().chain(ours));
        },
    );
}

/// udev's rules for the devices `cores` reach, from their manifests' matches:
/// each device's HID nodes, by its USB ids, opened by the person at the seat
/// (`uaccess`), as Ectomatter opens them; each device once, named by its core.
fn udev_rules(cores: &[packages::Package]) -> String {
    let mut out = String::from("# Ectomatter: the person at the computer may open the devices its cores reach\n# (written by `ectomatter --udev-rules`, from each core's manifest)\n");
    let mut seen = std::collections::BTreeSet::new();
    for p in cores {
        // (each device once, the rule a core added later gets of its own: permission.rs)
        let new: Vec<(String, String)> = permission::devices(&p.manifest.json()).into_iter().filter(|d| seen.insert(d.clone())).collect();
        out += &permission::rule(&p.manifest.name, p.id(), &new);
    }
    out
}

#[cfg(test)]
mod udev_tests {
    /// The rules name every device every core here reaches, by the ids its
    /// manifest gives, and no device twice.
    #[test]
    fn the_rules_open_every_cores_devices() {
        let cores = crate::packages::of(crate::packages::Kind::Core);
        assert!(!cores.is_empty(), "the source has cores");
        let rules = super::udev_rules(&cores);
        for p in &cores {
            for m in &p.manifest.spec.matches {
                let line = format!("ATTRS{{idVendor}}==\"{}\", ATTRS{{idProduct}}==\"{}\"", m.usb.vendor.to_lowercase(), m.usb.product.to_lowercase());
                assert_eq!(rules.matches(&line).count(), 1, "{} {line} in:\n{rules}", p.id());
            }
        }
        assert!(rules.lines().filter(|l| l.starts_with("SUBSYSTEM")).all(|l| l.ends_with("TAG+=\"uaccess\"")));
    }
}

/// A stop asked for by a signal, for the window's loop to act on.
static TERM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
extern "C" fn on_term(_: libc::c_int) {
    TERM.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let snapshot = args.iter().position(|a| a == "--snapshot").and_then(|i| args.get(i + 1)).cloned();
    // (a snapshot or a read of the keyboard is a tool's run, not the app's, and
    // leaves the person's log as it was)
    // `--udev-rules`: the rules that let the person logged in open every
    // device a core here reaches (an install puts them in udev's folder), and stop
    if args.iter().any(|a| a == "--udev-rules") {
        print!("{}", udev_rules(&packages::of(packages::Kind::Core)));
        return;
    }
    if snapshot.is_none() && !args.iter().any(|a| a == "--read-state") {
        // (one Ectomatter for each person: opened again, the one running comes forward,
        // before this one's log could take the place of its)
        let after = args.iter().position(|a| a == "--after").and_then(|i| args.get(i + 1)).and_then(|p| p.parse().ok());
        if once::running_already(args.iter().any(|a| a == "--hide"), after) {
            return;
        }
        log::start();
        // (an entry to start at login that names a program gone names this one, and
        // waits for KWin: autostart.rs)
        autostart::keep();
    }
    // ECTOMATTER_NOW=seconds since 1970: the time of day a snapshot is drawn
    // at, its animations stepped by frames (clock.rs), from before the window
    // keeps any time
    if snapshot.is_some()
        && let Some(t) = std::env::var("ECTOMATTER_NOW").ok().and_then(|t| t.parse().ok())
    {
        clock::fix(Duration::from_secs(t));
    }
    // what Ectomatter kept when it was named Ectodeck, moved to its own folders first
    old_name::move_old_home();
    // the typefaces legends can be set in: Ectomatter's own, the core's and
    // the person's added, loaded in the background so the lab's list of
    // them opens at once; and a typeface a setup names from before Ectomatter
    // kept its own, taken in from this computer
    if let Some(core) = cores::keyboard() {
        ectomatter_draw::print::add_fonts(core.fonts());
    }
    ectomatter_draw::print::add_fonts(typefaces::files());
    if let Some(core) = cores::keyboard() {
        for name in typefaces::adopt_named(&setups_typefaces(core)) {
            eprintln!("ectomatter: the typeface {name} taken into Ectomatter's own from this computer");
        }
        // the person's own pictures, put on keys in place of their text
        setups::load_images(&core.id);
    }
    ectomatter_draw::print::warm();
    // where its license and notices are, said in the log beside where its packages are
    about::folder();
    // `--read-state FILE`: read the keyboard's state once, for the snapshots, and stop
    if let Some(path) = args.iter().position(|a| a == "--read-state").and_then(|i| args.get(i + 1)) {
        // (the keyboard's core alone, started as any core is)
        let mut hosted = hosted::Hosted::new(std::sync::Arc::new(computer::ThisComputer::new()));
        if let Some(p) = packages::of(packages::Kind::Core).into_iter().find(|p| cores::keyboard().is_some_and(|k| k.id == p.id())) {
            hosted.start(p);
        }
        let kb = cores::keyboard().and_then(|k| hosted.drive_id(&k.id)).map(|(_, core)| core).ok_or_else(|| "Ectomatter has no keyboard's core".to_string());
        let state = kb.as_ref().map_err(|e| e.clone()).and_then(|k| {
            let s = k.call("device/read", &serde_json::Value::Null);
            let _ = k.call("device/release", &serde_json::Value::Null);
            s
        });
        match state {
            Ok(s) => std::fs::write(path, serde_json::to_vec(&s).unwrap()).unwrap(),
            Err(e) => eprintln!("ectomatter: the keyboard could not be read: {e}"),
        }
        return;
    }

    let made = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(Platform { made: made.clone(), start: Instant::now() })).unwrap();
    glyph_fonts_in_windows();
    let ui = AppWindow::new().unwrap();
    ink_env::apply(&ui);
    let _ = slint::ComponentHandle::window(&ui);
    let window = made.borrow().last().cloned().expect("the main window's adapter");
    ui.set_mark(glyph(include_str!("../ui/glyphs/mark.svg"), 16, false));
    // (and on its tile in Settings' About, at its own colors)
    ui.set_mark_tile(glyph(include_str!("../ui/glyphs/mark.svg"), 88, false));
    // (About's mark, whole at its own colors, drawn at twice its size to stay sharp on a dense screen)
    ui.set_about_mark(glyph(include_str!("../ui/icons/ectomatter.svg"), 2 * crate::tokens::size::MARK as u32, false));
    ui.set_home_glyph(glyph(include_str!("../ui/glyphs/home.svg"), 22, true));
    ui.set_keyboard_glyph(glyph(include_str!("../ui/glyphs/keyboard.svg"), 22, true));
    ui.set_settings_glyph(glyph(include_str!("../ui/glyphs/settings.svg"), 22, true));
    ui.set_library_glyph(glyph(include_str!("../ui/glyphs/library.svg"), 22, true));
    ui.set_keyboard_tile(glyph(include_str!("../ui/glyphs/keyboard.svg"), 88, true));
    ui.set_mouse_glyph(glyph(include_str!("../ui/glyphs/mouse.svg"), 22, true));
    // (the scenes' glyph, the Library's Scene row's: a light's ring and its rays, as Home's Scenes button has it)
    ui.set_scene_glyph(glyph(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"><path d="M7 4.6a2.4 2.4 0 1 0 0 4.8a2.4 2.4 0 1 0 0-4.8zM7 0.9v1.5M7 11.6v1.5M0.9 7h1.5M11.6 7h1.5M2.7 2.7l1.06 1.06M10.24 10.24l1.06 1.06M2.7 11.3l1.06-1.06M10.24 3.76l1.06-1.06"/></svg>"##, 22, true));
    ui.set_mouse_tile(glyph(include_str!("../ui/glyphs/mouse.svg"), 88, true));
    ui.set_rail_open(uistate::get("rail_open").and_then(|v| v.as_bool()).unwrap_or(false));
    slint::ComponentHandle::show(&ui).unwrap();

    let settings = settings::Settings::load(config_dir().join("settings.json"));
    // (each package's own files where they're kept now that cores and plugins are apart)
    if snapshot.is_none() {
        packages::move_old_folders();
    }
    let (reference, glow) = levels(&settings);
    let gain = LIGHT_GAIN;
    eprintln!("ectomatter: the desktop's white is {reference} nits; the light goes to {glow:.0}");

    if let Some(path) = snapshot {
        return snap(&window, &made, ui, reference, gain, &path);
    }
    // back on the place Ectomatter was started again from, for a change to what it may use
    if let Some(p) = args.iter().position(|a| a == "--place").and_then(|i| args.get(i + 1)) {
        ui.set_place(p.as_str().into());
    }
    // the window's shell: Wayland's on Linux, winit's where asked for
    // (ECTOMATTER_SHELL=winit) and on every other system
    #[cfg(target_os = "linux")]
    if !std::env::var("ECTOMATTER_SHELL").is_ok_and(|s| s == "winit") {
        return shell::wayland::run(window, made, ui, settings, reference, gain);
    }
    shell::winit::run(window, made, ui, settings, reference, gain);
}

/// The apps a snapshot's setting names come to the front, in order, as KWin
/// tells it: each `class,desktop` (and its process, `class,desktop,pid`),
/// apart by semicolons.
fn front_apps_told(front: &mut Front, var: &str) {
    for f in std::env::var(var).unwrap_or_default().split(';').filter(|f| !f.is_empty()) {
        let mut parts = f.splitn(3, ',');
        let (class, desktop) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let pid = parts.next().and_then(|p| p.trim().parse().ok()).unwrap_or(0);
        front.front_app(front_app::Front { class: class.into(), title: String::new(), desktop: desktop.into(), pid, ..Default::default() });
    }
}

/// A snapshot waits for the packages to go still: until nothing new has
/// been heard from any (a reading's value, a device, a warning) for a second
/// and a half, or twenty seconds have gone. It waits in real time and lets
/// no frame pass, so the window's animations stand where they were; a slower
/// computer waits longer and draws the same picture.
fn settle(front: &mut Front) {
    let (start, mut last, mut since) = (Instant::now(), front.hosted.heard, Instant::now());
    while since.elapsed() < Duration::from_millis(1500) && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(clock::FRAME);
        // (what the window's tick would ask: the packages heard, and told what's wanted of them)
        front.hosted.poll();
        front.act();
        if front.hosted.heard != last {
            (last, since) = (front.hosted.heard, Instant::now());
        }
    }
}

/// One frame without a window, saved for checking.
fn snap(window: &Rc<MinimalSoftwareWindow>, made: &Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, ui: AppWindow, reference: f64, gain: f32, path: &str) {
    let (mut w, mut h) = std::env::var("ECTOMATTER_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or(SIZE);
    window.set_size(PhysicalSize::new(w, h));
    let settings = settings::Settings::load(config_dir().join("settings.json"));
    // ECTOMATTER_COMPUTER=FOLDER: the computer the packages reach is the one
    // described there (computer::Described), not this one
    let this: std::sync::Arc<dyn core_host::Computer> = match std::env::var_os("ECTOMATTER_COMPUTER") {
        Some(f) => {
            computer::describe(f.clone().into());
            std::sync::Arc::new(computer::Described(f.into()))
        }
        None => std::sync::Arc::new(computer::ThisComputer::new()),
    };
    let mut hosted = hosted::Hosted::new(this);
    hosted.start_all(&settings);
    devices_described(&hosted);
    let mut front = Front::new(ui, hosted, None, true, settings);
    front.ask_credits();
    front.levels.0 = reference;
    if let Some(state) = std::env::var_os("ECTOMATTER_STATE") {
        // (as the keyboard's core reads it, device/read)
        let dev: serde_json::Value = serde_json::from_slice(&std::fs::read(state).expect("the saved state")).expect("the saved state as JSON");
        front.heard(device::Heard::State(Ok(dev)));
    }
    // (and then, or instead, a read that failed)
    if let Ok(e) = std::env::var("ECTOMATTER_UNREAD") {
        front.heard(device::Heard::State(Err(e)));
    }
    // (the packages' first answers in: their devices, sections and readings)
    settle(&mut front);
    // the screen as the player would have it: this computer's Stats, or
    // ECTOMATTER_LCD's rows (split by |), in ECTOMATTER_LCD_STATE
    // (playing, paused, missing, launch, exit)
    {
        let rows = std::env::var("ECTOMATTER_LCD").map(|r| r.split('|').map(String::from).collect()).unwrap_or_else(|_| {
            // (asked for once, so the plugins read them, and read again once they've answered)
            let mut stats = lcd_stats::Stats::of(front.hosted.readings(), "snapshot");
            let _ = stats.rows();
            settle(&mut front);
            stats.rows()
        });
        eprintln!("the screen's rows: {rows:?}");
        let st = std::env::var("ECTOMATTER_LCD_STATE").unwrap_or_default();
        let state = match st.as_str() {
            "paused" => lcd_player::State::Paused,
            "missing" => lcd_player::State::Missing,
            _ => lcd_player::State::Playing,
        };
        let sequence = match st.as_str() {
            "launch" => Some("launch"),
            "exit" => Some("exit"),
            _ => None,
        };
        // ECTOMATTER_LCD_PAGE: which page of the playlist shows
        let page = std::env::var("ECTOMATTER_LCD_PAGE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        // ECTOMATTER_LCD_AT=s or turning,s,length: where the playlist stands in the page's
        // turn (by default three seconds into an eight-second page)
        let at: Vec<f64> = std::env::var("ECTOMATTER_LCD_AT").unwrap_or_default().split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let (turning, elapsed, length) = match at.as_slice() {
            [t, e, l] => (*t > 0.5, *e, *l),
            [e] => (false, *e, 8.0),
            _ => (false, 3.0, 8.0),
        };
        front.screen_news(lcd_player::News { rows, sequence, state, page, turning, elapsed, length });
    }
    // ECTOMATTER_LIBRARY=FILE: Ectomatter's own macros, as macros.json keeps them
    if let Some(v) = std::env::var("ECTOMATTER_LIBRARY").ok().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        library::seed(&v);
    }
    // ECTOMATTER_PADS=FILE: the pads on Home's desk, as window.json keeps them
    // (kept nowhere), and ECTOMATTER_PAD_PICTURE=FILE the picture "Choose a
    // picture…" takes, as if chosen in the desktop's file picker
    if let Some(v) = std::env::var("ECTOMATTER_PADS").ok().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        front.desk.pads = desk::pads_of(&v);
        // (fitted again with them, as the window fits the pads it reads as it
        // starts, and their pictures read, at once offscreen)
        front.desk.fit();
        front.poll_pads();
    }
    // what the keyboard's core says of it, the Lighting page's effects among it
    // (as its core, started offscreen with no device, describes it)
    front.page.desc = cores::keyboard().and_then(|k| front.hosted.get(&k.id)).map(|r| r.describes.clone()).unwrap_or_default();
    // ECTOMATTER_LIGHT_AT=ms: the keys' light at that moment of its effect
    front.light_at = std::env::var("ECTOMATTER_LIGHT_AT").ok().and_then(|v| v.parse().ok());
    // ECTOMATTER_MOUSE=FILE: the mouse as its core last told it (what the mouse's
    // thread hears), and ECTOMATTER_MOUSE_TAB=N its page's tab
    if let Some(v) = std::env::var("ECTOMATTER_MOUSE").ok().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()) {
        front.mouse_heard(mouse::Heard::State(v));
    }
    if let Some(t) = std::env::var("ECTOMATTER_MOUSE_TAB").ok().and_then(|t| t.parse().ok()) {
        front.mouse.tab = t;
    }
    if let Ok(place) = std::env::var("ECTOMATTER_PLACE") {
        // ("sensors": Settings, at the sensors' box, as the screen editor's links open it)
        if place == "sensors" {
            front.show_sensors();
        } else {
            front.ui.set_place(place.into());
            // (the place's own cells built, so clicks land on them)
            front.refresh();
            // ECTOMATTER_DESK=edit: Home's desk in edit mode, its dots shown, before the clicks
            if std::env::var("ECTOMATTER_DESK").is_ok_and(|v| v == "edit") {
                front.desk.set_editing(true);
                front.desk.tick(Instant::now() + Duration::from_secs(1));
                front.refresh();
            }
        }
    }
    // ECTOMATTER_MACROS=N: the Macro Editor's window instead, open on macro N
    // (with the mouse's profiles ECTOMATTER_PROFILES names, for its actions)
    if let Some(n) = std::env::var("ECTOMATTER_MACROS").ok().and_then(|v| v.parse::<u32>().ok()) {
        if let Some(s) = std::env::var_os("ECTOMATTER_PROFILES").and_then(|p| profiles::Store::read_only(p.into())) {
            front.mouse.profiles = Some(s);
        }
        return snap_macros(front, made, n, path);
    }
    // ECTOMATTER_LAYOUTS=N: the screen layout editor's window instead, open on layout N
    if let Some(n) = std::env::var("ECTOMATTER_LAYOUTS").ok().and_then(|v| v.parse::<usize>().ok()) {
        return snap_layouts(front, made, n, path);
    }
    // ECTOMATTER_LAB: the lab's window instead
    if std::env::var_os("ECTOMATTER_LAB").is_some() {
        return snap_lab(front, made, reference, path);
    }
    // lay the page out once, so clicks land where it draws
    let mut pic = Picture::new(reference, ink_env::rgb("stage", STAGE));
    pic.resize(w as usize, h as usize);
    window.draw_if_needed(|r| {
        r.render(&mut pic.ui, w as usize);
    });
    // ECTOMATTER_PROFILES=FILE: the mouse's profiles as that file keeps them (kept nowhere)
    if let Some(s) = std::env::var_os("ECTOMATTER_PROFILES").and_then(|p| profiles::Store::read_only(p.into())) {
        front.mouse.profiles = Some(s);
    }
    // ECTOMATTER_FRONT=class,desktop;...: apps come to the front, in order, as KWin tells it
    front_apps_told(&mut front, "ECTOMATTER_FRONT");
    // ECTOMATTER_SCENE=id: that scene on (not kept); ECTOMATTER_SCENE_FORM=new, or a
    // scene's id: the scene form open, making one or changing that one
    if let Ok(id) = std::env::var("ECTOMATTER_SCENE") {
        front.set_scene(Some(id), false);
    }
    if let Ok(f) = std::env::var("ECTOMATTER_SCENE_FORM") {
        front.open_scene_form((f != "new").then_some(f));
    }
    for click in std::env::var("ECTOMATTER_CLICKS").unwrap_or_default().split(';').filter(|c| !c.is_empty()) {
        let mut xy = click.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        let (x, y) = (xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0));
        let position = LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        // ECTOMATTER_SETTLE: the pointer rests a frame before each press, as a hand
        // does, so what shows only under the pointer (a block's buttons) is there to press
        if std::env::var_os("ECTOMATTER_SETTLE").is_some() {
            front.act();
            window.draw_if_needed(|r| {
                r.render(&mut pic.ui, w as usize);
            });
        }
        front.pressed_at(x, y);
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        // ECTOMATTER_MIDCLICK: the page refreshed between the press and the
        // release, as a reading from the keyboard does ten times a second
        if std::env::var_os("ECTOMATTER_MIDCLICK").is_some() {
            front.refresh();
            window.draw_if_needed(|r| {
                r.render(&mut pic.ui, w as usize);
            });
        }
        window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        front.act();
        window.draw_if_needed(|r| {
            r.render(&mut pic.ui, w as usize);
        });
    }
    // ECTOMATTER_FRONT_AFTER: the same, after the clicks
    front_apps_told(&mut front, "ECTOMATTER_FRONT_AFTER");
    // (the scene, if one plays, at a moment of its own: ECTOMATTER_SCENE_AT=seconds)
    if let Some(t) = std::env::var("ECTOMATTER_SCENE_AT").ok().and_then(|v| v.parse::<f64>().ok()) {
        std::thread::sleep(Duration::from_secs_f64(t));
    }
    front.scene_now();
    // ECTOMATTER_HEAR=control: a mouse button heard pressed and let go, as its core tells
    // it; ECTOMATTER_KEY_CODE=code: a keyboard key with that host code heard so (after
    // the clicks, each job Ectomatter does said in the log)
    if let Ok(c) = std::env::var("ECTOMATTER_HEAR") {
        front.mouse_heard(mouse::Heard::Input(c.clone(), true));
        front.mouse_heard(mouse::Heard::Input(c, false));
        front.act();
    }
    if let Some(code) = std::env::var("ECTOMATTER_KEY_CODE").ok().and_then(|v| v.parse::<u32>().ok()) {
        // (as the keyboard's core tells it: the key's host slot, by its code)
        let slot = code.saturating_sub(keyboard::NEXT_LAYOUT);
        for down in [true, false] {
            front.heard(device::Heard::Input(serde_json::json!({ "device": cores::keyboard_key(), "host": slot, "sends": { "host": slot } }), down));
        }
        front.act();
    }
    // ECTOMATTER_WHEEL="x,y,dy": the wheel turned there, after the clicks
    if let Some(v) = std::env::var("ECTOMATTER_WHEEL").ok().map(|v| v.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 3) {
        let position = LogicalPosition::new(v[0], v[1]);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: v[2] });
        front.act();
    }
    // ECTOMATTER_RCLICK="x,y": a click with the right button, after the others
    if let Some((x, y)) = std::env::var("ECTOMATTER_RCLICK").ok().and_then(|s| s.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)))) {
        let position = LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        front.pressed_at(x, y);
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Right });
        window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Right });
        front.act();
        window.draw_if_needed(|r| {
            r.render(&mut pic.ui, w as usize);
        });
    }
    // ECTOMATTER_DRAG_DROP="x0,y0;x1,y1": pressed at the first point, dragged to the second, let go
    // (ECTOMATTER_DRAG_LIVE: the window acting on each move and drawn after it, as it is under
    // a hand, so what a move changes is there for the next)
    let drag: Vec<f32> = std::env::var("ECTOMATTER_DRAG_DROP").unwrap_or_default().split([',', ';']).filter_map(|v| v.trim().parse().ok()).collect();
    if let [x0, y0, x1, y1] = drag[..] {
        let (from, to) = (LogicalPosition::new(x0, y0), LogicalPosition::new(x1, y1));
        let live = std::env::var_os("ECTOMATTER_DRAG_LIVE").is_some();
        window.dispatch_event(WindowEvent::PointerMoved { position: from });
        window.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
        for k in 1..=8 {
            let f = k as f32 / 8.0;
            window.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f) });
            if live {
                slint::platform::update_timers_and_animations();
                front.act();
                window.draw_if_needed(|r| {
                    r.render(&mut pic.ui, w as usize);
                });
            }
        }
        // ECTOMATTER_DRAG_HOLD: still held at the second point, to see the drag under way
        if std::env::var_os("ECTOMATTER_DRAG_HOLD").is_none() {
            window.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });
        }
        front.act();
        window.draw_if_needed(|r| {
            r.render(&mut pic.ui, w as usize);
        });
    }
    for c in std::env::var("ECTOMATTER_TYPE").unwrap_or_default().chars() {
        let text: SharedString = c.to_string().into();
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        slint::platform::update_timers_and_animations();
        front.act();
    }
    // ECTOMATTER_PAUSE=ms: that long passes after the typing, before the clicks after
    // it (what was typed looked up, say)
    if let Some(ms) = std::env::var("ECTOMATTER_PAUSE").ok().and_then(|v| v.parse::<u64>().ok()) {
        for _ in 0..clock::frames(ms) {
            clock::frame();
            slint::platform::update_timers_and_animations();
            front.act();
        }
    }
    // ECTOMATTER_CLICKS_AFTER: clicks once the typing is done ("x,y;x,y")
    for click in std::env::var("ECTOMATTER_CLICKS_AFTER").unwrap_or_default().split(';').filter(|c| !c.is_empty()) {
        let mut xy = click.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        let (x, y) = (xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0));
        let position = LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        // (as the window tells the page of every press, before the press itself)
        front.pressed_at(x, y);
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        front.act();
        window.draw_if_needed(|r| {
            r.render(&mut pic.ui, w as usize);
        });
    }
    // ECTOMATTER_POINTER_AWAY: the pointer gone off the window after the clicks, so
    // nothing the last one pressed shows as under it
    if std::env::var_os("ECTOMATTER_POINTER_AWAY").is_some() {
        window.dispatch_event(WindowEvent::PointerExited);
        front.act();
        window.draw_if_needed(|r| {
            r.render(&mut pic.ui, w as usize);
        });
    }
    // ECTOMATTER_ESCAPE=N: Escape pressed N times after those clicks; ECTOMATTER_DELETE:
    // then the Delete key
    let escapes = std::env::var("ECTOMATTER_ESCAPE").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    let delete = std::env::var_os("ECTOMATTER_DELETE").is_some();
    for k in 0..escapes + delete as u32 {
        if k < escapes {
            front.escape();
        } else {
            front.desk_delete_key();
        }
        front.act();
        window.draw_if_needed(|r| {
            r.render(&mut pic.ui, w as usize);
        });
    }
    // the wheel turned over the page, in logical pixels
    if let Some(dy) = std::env::var("ECTOMATTER_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
        // where the wheel turns: ECTOMATTER_SCROLL_AT="x,y", or the page's middle
        let at: Vec<f32> = std::env::var("ECTOMATTER_SCROLL_AT").unwrap_or_default().split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let position = if let [x, y] = at[..] { LogicalPosition::new(x, y) } else { LogicalPosition::new(700.0, 600.0) };
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: -dy });
        for _ in 0..clock::frames(400) {
            clock::frame();
            slint::platform::update_timers_and_animations();
        }
    }
    // keys typed on the keyboard, as Linux key codes: "29+" down, "29-" up
    // keys as the keyboard sends them: into the window, and as its core tells of them
    for k in std::env::var("ECTOMATTER_EVDEV").unwrap_or_default().split(',').filter(|k| !k.is_empty()) {
        let (code, down) = (k.trim_end_matches(['+', '-']).parse().unwrap_or(0), k.ends_with('+'));
        front.typed(code, down);
        // (what the key sent, as its core reads it from the keyboard's report)
        if let Some((mods, hid)) = keyboard::evdev_to_hid(code) {
            let usage = if hid != 0 { hid as u8 } else { ectomatter_protocol::keys::FIRST_MODIFIER + mods.trailing_zeros() as u8 };
            front.heard(device::Heard::Input(serde_json::json!({ "device": cores::keyboard_key(), "sends": ectomatter_protocol::Action::Key(usage).write() }), down));
        }
        front.refresh();
    }
    // ECTOMATTER_SCREEN_KEYS="167,169": keys with screen jobs pressed, by their codes, as
    // the keyboard's reports would tell of them
    for code in std::env::var("ECTOMATTER_SCREEN_KEYS").unwrap_or_default().split(',').filter_map(|c| c.trim().parse::<u32>().ok()) {
        front.screen_key(code);
        front.refresh();
    }
    // ECTOMATTER_SLIDERS='{"dimmer":64,...}': a reading of the sliders, as the keyboard sends one
    if let Some(r) = std::env::var("ECTOMATTER_SLIDERS").ok().and_then(|v| serde_json::from_str(&v).ok()) {
        front.heard(device::Heard::Sliders(Ok(r)));
    }
    // ECTOMATTER_ACTIVE=N: the keyboard reports it is using profile N, as when it is switched on the keyboard
    if let Some(n) = std::env::var("ECTOMATTER_ACTIVE").ok().and_then(|v| v.parse::<u8>().ok()) {
        front.heard(device::Heard::ActiveNow(Ok(Some(n))));
    }
    // ECTOMATTER_LOCKS=N: the keyboard's lock lights (1 Num Lock, 2 Caps Lock, 4 Scroll Lock)
    if let Some(bits) = std::env::var("ECTOMATTER_LOCKS").ok().and_then(|v| v.parse::<u8>().ok()) {
        front.locks(bits);
    }
    // the pointer, where it was last left
    if let Ok(at) = std::env::var("ECTOMATTER_HOVER") {
        let mut xy = at.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        window.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0)) });
        // (what the pointer's being there asks, done, as the window does at once)
        front.act();
    }
    // ECTOMATTER_DRAG="x,y;x,y": a press dragged from the first point to the
    // second in 30 steps, each step timed as the window would handle it
    if let Ok(d) = std::env::var("ECTOMATTER_DRAG") {
        let pts: Vec<(f32, f32)> = d
            .split(';')
            .filter_map(|p| p.split_once(',').and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?))))
            .collect();
        if let [(x0, y0), (x1, y1)] = pts[..] {
            let at = |t: f32| LogicalPosition::new(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            // ECTOMATTER_DRAG_BUTTON=middle or right: the drag made with that button
            let button = match std::env::var("ECTOMATTER_DRAG_BUTTON").as_deref() {
                Ok("middle") => PointerEventButton::Middle,
                Ok("right") => PointerEventButton::Right,
                _ => PointerEventButton::Left,
            };
            window.dispatch_event(WindowEvent::PointerMoved { position: at(0.0) });
            window.dispatch_event(WindowEvent::PointerPressed { position: at(0.0), button });
            let rect = board_rect(&front.ui, 1.0);
            let board = cores::keyboard().filter(|_| rect.2 > 0).map(|core| settled(&core.tree(), &device_in_use(core, &front.settings), ink_env::rgb("stage", STAGE), rect.2 as u32, rect.3 as u32));
            for k in 1..=30 {
                let t0 = Instant::now();
                window.dispatch_event(WindowEvent::PointerMoved { position: at(k as f32 / 30.0) });
                let t1 = Instant::now();
                front.act();
                let t2 = Instant::now();
                window.draw_if_needed(|r| {
                    r.render(&mut pic.ui, w as usize);
                });
                let t3 = Instant::now();
                let lights = front.lights();
                let t4 = Instant::now();
                if let (Some(b), Some(l)) = (&board, &lights) {
                    let _ = light(b, l, 0, gain, reference, None);
                    pic.encode(rect.0.max(0) as usize, rect.1.max(0) as usize, rect.2, rect.3);
                }
                let t5 = Instant::now();
                eprintln!("step {k}: event {:?} act {:?} page {:?} lights {:?} keyboard {:?}", t1 - t0, t2 - t1, t3 - t2, t4 - t3, t5 - t4);
            }
            window.dispatch_event(WindowEvent::PointerReleased { position: at(1.0), button });
            front.act();
        }
    }
    // let animations run their course, as the eye would see them settle
    if let Some(ms) = std::env::var("ECTOMATTER_WAIT").ok().and_then(|v| v.parse::<u64>().ok()) {
        for _ in 0..clock::frames(ms) {
            clock::frame();
            slint::platform::update_timers_and_animations();
            front.act();
        }
        // (and what the clicks asked of the packages answered)
        settle(&mut front);
    }
    eprintln!("the cursor: {:?}", window.pointer());
    // (the caps' colors, for what is printed over the keys)
    if let Some(core) = cores::keyboard() {
        front.light_caps(&device_in_use(core, &front.settings)["light"]);
    }
    front.refresh();
    // (About's Blob, its frame at the moment the animations' clock has reached)
    front.draw_blob(1.0);
    window.request_redraw();
    window.draw_if_needed(|r| {
        r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
        r.render(&mut pic.ui, w as usize);
    });
    let rect = board_rect(&front.ui, 1.0);
    front.board_size(rect.2 as u32, rect.3 as u32, 1.0);
    window.request_redraw();
    window.draw_if_needed(|r| {
        r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
        r.render(&mut pic.ui, w as usize);
    });
    lay_ground(&mut pic, &front.ui, 1.0);
    if let Some(core) = cores::keyboard().filter(|_| rect.2 > 0) {
        let tree = core.tree();
        let device = device_in_use(core, &front.settings);
        let t = Instant::now();
        let mut board = settled(&tree, &device, ink_env::rgb("stage", STAGE), rect.2 as u32, rect.3 as u32);
        // its shadow, as the window lays it
        board.lay_shadow(&stage_shadow(1.0), STAGE_SHADOW_OPACITY);
        if let Some(s) = &board.cast {
            front.ui.set_board_shadow(shadow_image(s, board.w, board.h));
            front.ui.set_board_shadow_margin(s.margin as f32);
        }
        eprintln!("keyboard drawn and its light simulated in {:?}", t.elapsed());
        let lights = front.lights().unwrap_or_default();
        let gain = front.light_gain(reference).max(gain);
        eprintln!("the light at {:.0} nits", gain as f64 * reference);
        let mut lit = light(&board, &lights, front.page.locks, gain, reference, front.overlays.panel_for(board.w, board.lcd().dot));
        if let Some(rects) = front.dim_rects(board.w) {
            dim_around(&mut lit, board.w as usize, &rects, board.chassis());
        }
        let lit = Rc::new(Lit { px: lit, w: rect.2, h: rect.3 });
        lay_keyboard(&mut pic, &front, false, Some(&lit), None, 1.0);
        front.ui.set_board_ready(true);
        window.request_redraw();
        window.draw_if_needed(|r| {
            r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            r.render(&mut pic.ui, w as usize);
        });
        // ECTOMATTER_RESIZE=WxH: the window made that size as the window does
        // mid-resize: the page laid out and drawn anew, the keyboard's last
        // picture stretched to its new place until a new one is drawn
        if let Some((nw, nh)) = std::env::var("ECTOMATTER_RESIZE").ok().and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))) {
            window.set_size(PhysicalSize::new(nw, nh));
            pic.resize(nw as usize, nh as usize);
            let r2 = board_rect(&front.ui, 1.0);
            front.board_size(r2.2 as u32, r2.3 as u32, 1.0);
            slint::platform::update_timers_and_animations();
            window.request_redraw();
            window.draw_if_needed(|r| {
                r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
                r.render(&mut pic.ui, nw as usize);
            });
            lay_ground(&mut pic, &front.ui, 1.0);
            lay_keyboard(&mut pic, &front, false, Some(&lit), None, 1.0);
            eprintln!("resized to {nw}x{nh}: keyboard now at {:?}, its picture still {}x{}", pic.keyboard_rect(), lit.w, lit.h);
            if std::env::var_os("ECTOMATTER_DEBUG").is_some() {
                use slint::Model;
                let m = front.ui.get_overlays();
                for i in 0..m.row_count() {
                    let o = m.row_data(i).unwrap();
                    eprintln!("  overlay {i}: x {:.1} y {:.1} w {:.1} h {:.1} travel {:.1} frac {:.2}", o.x, o.y, o.w, o.h, o.travel, o.frac);
                }
            }
            (w, h) = (nw, nh);
        }
    }
    // Home's desk: the keyboard on it drawn for its place there, on the app's
    // ground, and laid as the window lays it
    if let (Some([x, y, kw, kh]), Some(core)) = (front.desk_keyboard(), cores::keyboard()) {
        let (bw, bh) = (kw.round().max(1.0) as u32, kh.round().max(1.0) as u32);
        let tree = core.tree();
        let device = device_in_use(core, &front.settings);
        let mut board = settled(&tree, &device, ink_env::rgb("base", HOME_GROUND), bw, bh);
        board.lay_shadow(&stage_shadow(1.0), STAGE_SHADOW_OPACITY);
        if let Some(s) = &board.cast {
            front.ui.set_desk_kb_shadow(shadow_image(s, board.w, board.h));
            front.ui.set_desk_kb_shadow_m(s.margin as f32);
        }
        front.board_size(bw, bh, 1.0);
        front.ui.set_desk_kb_k(1.0);
        let on = front.desk_keyboard_on();
        let lights = if on { front.lights().unwrap_or_default() } else { Vec::new() };
        let gain = front.light_gain(reference).max(gain);
        let lit = light(&board, &lights, front.page.locks, gain, reference, front.overlays.panel_for(board.w, board.lcd().dot));
        let lit = Rc::new(Lit { px: lit, w: bw as usize, h: bh as usize });
        lay_keyboard(&mut pic, &front, true, Some(&lit), Some(Rc::new(board.ground_share())), 1.0);
        front.ui.set_board_ready(true);
        eprintln!("Home's keyboard drawn at {bw}x{bh}, at {x},{y}; laid at {:?}", pic.keyboard_rect());
    } else {
        // (what the desk lays on the ground, with no keyboard on it)
        pic.set_decor(front.desk_decor(1.0));
    }
    // the mouse in its page's hole, or on Home's desk at its size there, lit as the window lights it
    front.levels.0 = reference;
    if front.ui.get_place() == "home" {
        front.ready_desk_mouse();
    }
    // ECTOMATTER_HOLD="j;mouse/right": those keys (by the core's ids) and mouse
    // buttons (by the core's) held down, as the devices' presses hold them
    // (the ripples their presses sent are ECTOMATTER_RIPPLE's)
    for id in std::env::var("ECTOMATTER_HOLD").unwrap_or_default().split(';').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some(button) = id.strip_prefix("mouse/") {
            front.mouse_button_held(button, true);
        } else if let Some(pos) = front.page.layout.iter().find(|k| k.id == id).map(|k| k.pos) {
            front.page.pressed.insert(pos);
            front.refresh();
        }
    }
    lay_mouse(&mut pic, &mut front, 1.0);
    if pic.mouse_rect().2 > 0 {
        eprintln!("the mouse laid at {:?}", pic.mouse_rect());
    }
    // ECTOMATTER_RIPPLE="f,400;enter,900;mouse/right,600": ripples from those
    // keys and mouse buttons (by their cores' ids), each that
    // many milliseconds into its run, as pressing them sends across Home's
    // desk, in the devices' light as they showed when each was pressed: with
    // ECTOMATTER_LIGHT_AT, at that moment less its age (so a moving picture's
    // ripple keeps the light it started in, as the window's does)
    let pinned = front.light_at;
    let mut rings = Vec::new();
    for part in std::env::var("ECTOMATTER_RIPPLE").unwrap_or_default().split(';').filter(|p| !p.is_empty()) {
        let (id, ms) = part.split_once(',').unwrap_or((part, "0"));
        let age = Duration::from_millis(ms.trim().parse().unwrap_or(0));
        let from = match id.trim().strip_prefix("mouse/") {
            Some(button) => front.button_ripple_from(button),
            None => front.page.layout.iter().find(|k| k.id == id.trim()).and_then(|k| front.key_ripple_from(k.pos)),
        };
        let Some(at) = from else { continue };
        if let Some(t) = pinned {
            front.light_at = Some(t - age.as_millis() as f64);
            front.scene_now();
            let _ = front.lights();
        }
        rings.push((at, front.ripple_light(at), age));
    }
    if let Some(t) = pinned.filter(|_| !rings.is_empty()) {
        front.light_at = Some(t);
        front.scene_now();
        let _ = front.lights();
    }
    // (sent together, each its age into its run at the same moment)
    for (at, color, age) in rings {
        front.desk.ripple(at, color, age);
        pic.set_decor(front.desk_decor(1.0));
    }
    window.request_redraw();
    window.draw_if_needed(|r| {
        r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
        r.render(&mut pic.ui, w as usize);
    });
    let t = Instant::now();
    pic.encode_all();
    eprintln!("frame encoded in {:?}", t.elapsed());
    std::fs::write(path, bytemuck::cast_slice::<[u16; 4], u8>(&pic.out)).unwrap();
    // the white it was encoded for, which decoding it needs
    std::fs::write(format!("{path}.reference"), format!("{reference}\n")).unwrap();
    eprintln!("saved {path}: {w}x{h}, keyboard at {rect:?}");
}

/// Which of the Macro Editor's libraries a snapshot or a startup open is on:
/// ECTOMATTER_MACRO_LIB=ectomatter for Ectomatter's own, else the keyboard's.
fn macro_lib() -> macros::Lib {
    if std::env::var("ECTOMATTER_MACRO_LIB").is_ok_and(|v| v == "ectomatter") {
        macros::Lib::Ectomatter
    } else {
        macros::Lib::Keyboard
    }
}

/// The Macro Editor's window without a window: one frame, saved as a PNG.
fn snap_macros(mut front: Front, made: &Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, n: u32, path: &str) {
    let (w, h) = std::env::var("ECTOMATTER_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or(MACRO_SIZE);
    let ui = MacroWindow::new().unwrap();
    let _ = slint::ComponentHandle::window(&ui);
    let window = made.borrow().last().cloned().unwrap();
    slint::ComponentHandle::show(&ui).unwrap();
    window.set_size(PhysicalSize::new(w, h));
    front.macros = Some(macro_page::MacroPage::new(ui, glyph(include_str!("../ui/glyphs/mark.svg"), 16, false)));
    let mut px = vec![PremultipliedRgbaColor::default(); (w * h) as usize];
    let draw = |px: &mut Vec<PremultipliedRgbaColor>| {
        window.draw_if_needed(|r| {
            r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            r.render(px, w as usize);
        });
    };
    draw(&mut px);
    front.macros_elsewhere();
    if let Some(m) = &mut front.macros {
        m.open(&front.page, macro_lib(), (n > 0).then_some(n));
    }
    window.request_redraw();
    draw(&mut px);
    for click in std::env::var("ECTOMATTER_CLICKS").unwrap_or_default().split(';').filter(|c| !c.is_empty()) {
        let mut xy = click.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        let (x, y) = (xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0));
        let position = LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        if let Some(m) = &mut front.macros {
            m.pressed_at(x, y);
        }
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        front.act_macros();
        window.request_redraw();
        draw(&mut px);
    }
    for c in std::env::var("ECTOMATTER_TYPE").unwrap_or_default().chars() {
        let text: SharedString = if c == '\n' { Key::Return.into() } else { c.to_string().into() };
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        slint::platform::update_timers_and_animations();
        front.act_macros();
    }
    // ECTOMATTER_DRAG="x,y;x,y": a press at the first point, moved to the
    // second; let go there only with ECTOMATTER_DRAG_RELEASE
    if let Ok(d) = std::env::var("ECTOMATTER_DRAG") {
        let pts: Vec<(f32, f32)> = d
            .split(';')
            .filter_map(|p| p.split_once(',').and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?))))
            .collect();
        if let [(x0, y0), (x1, y1)] = pts[..] {
            let at = |t: f32| LogicalPosition::new(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            window.dispatch_event(WindowEvent::PointerMoved { position: at(0.0) });
            window.dispatch_event(WindowEvent::PointerPressed { position: at(0.0), button: PointerEventButton::Left });
            for k in 1..=10 {
                window.dispatch_event(WindowEvent::PointerMoved { position: at(k as f32 / 10.0) });
                front.act_macros();
            }
            if std::env::var_os("ECTOMATTER_DRAG_RELEASE").is_some() {
                window.dispatch_event(WindowEvent::PointerReleased { position: at(1.0), button: PointerEventButton::Left });
            }
        }
    }
    if let Ok(at) = std::env::var("ECTOMATTER_HOVER") {
        let mut xy = at.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        window.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0)) });
    }
    front.act_macros();
    window.request_redraw();
    draw(&mut px);
    let mut pix = Pixmap::new(w, h).unwrap();
    for (d, p) in pix.pixels_mut().iter_mut().zip(&px) {
        *d = resvg::tiny_skia::PremultipliedColorU8::from_rgba(p.red, p.green, p.blue, p.alpha).unwrap_or(resvg::tiny_skia::PremultipliedColorU8::TRANSPARENT);
    }
    pix.save_png(path).unwrap();
    eprintln!("saved {path}: {w}x{h}");
}

/// The screen layout editor's window without a window: one frame, saved as a
/// PNG, after the clicks, typing and drag the snapshot's settings ask for
/// (ECTOMATTER_CLICKS, ECTOMATTER_TYPE, ECTOMATTER_CLICKS_AFTER,
/// ECTOMATTER_DELETE for the Delete key, ECTOMATTER_DRAG_DROP with
/// ECTOMATTER_DRAG_HOLD to keep it held, ECTOMATTER_HOVER), and with
/// ECTOMATTER_CLOSE, closed.
fn snap_layouts(mut front: Front, made: &Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, n: usize, path: &str) {
    let (w, h) = std::env::var("ECTOMATTER_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or(LAYOUT_SIZE);
    let ui = LayoutWindow::new().unwrap();
    ui.set_device_name(cores::keyboard().map_or("", |c| c.name()).into());
    let _ = slint::ComponentHandle::window(&ui);
    let window = made.borrow().last().cloned().unwrap();
    slint::ComponentHandle::show(&ui).unwrap();
    window.set_size(PhysicalSize::new(w, h));
    front.layouts = Some(layout_page::LayoutPage::new(ui, glyph(include_str!("../ui/glyphs/mark.svg"), 16, false)));
    let mut px = vec![PremultipliedRgbaColor::default(); (w * h) as usize];
    let draw = |px: &mut Vec<PremultipliedRgbaColor>| {
        window.draw_if_needed(|r| {
            r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            r.render(px, w as usize);
        });
    };
    draw(&mut px);
    front.open_layouts(n);
    window.request_redraw();
    draw(&mut px);
    let clicks = |front: &mut Front, px: &mut Vec<PremultipliedRgbaColor>, list: &str| {
        for click in list.split(';').filter(|c| !c.is_empty()) {
            let mut xy = click.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
            let (x, y) = (xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0));
            let position = LogicalPosition::new(x, y);
            window.dispatch_event(WindowEvent::PointerMoved { position });
            if let Some(l) = &mut front.layouts {
                l.pressed_at(x, y);
            }
            window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
            window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
            front.act_layouts();
            window.request_redraw();
            draw(px);
        }
    };
    clicks(&mut front, &mut px, &std::env::var("ECTOMATTER_CLICKS").unwrap_or_default());
    // (\n is Return, and \u{8} Backspace)
    for c in std::env::var("ECTOMATTER_TYPE").unwrap_or_default().chars() {
        let text: SharedString = if c == '\n' { Key::Return.into() } else if c == '\u{8}' { Key::Backspace.into() } else { c.to_string().into() };
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        slint::platform::update_timers_and_animations();
        front.act_layouts();
    }
    clicks(&mut front, &mut px, &std::env::var("ECTOMATTER_CLICKS_AFTER").unwrap_or_default());
    if std::env::var_os("ECTOMATTER_DELETE").is_some() && !front.typing_in_layouts() {
        if let Some(l) = &front.layouts {
            l.delete_key();
        }
        front.act_layouts();
    }
    // a press at the first point, moved to the second in steps, let go there
    // unless ECTOMATTER_DRAG_HOLD
    let drag: Vec<f32> = std::env::var("ECTOMATTER_DRAG_DROP").unwrap_or_default().split([',', ';']).filter_map(|v| v.trim().parse().ok()).collect();
    if let [x0, y0, x1, y1] = drag[..] {
        let at = |t: f32| LogicalPosition::new(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
        window.dispatch_event(WindowEvent::PointerMoved { position: at(0.0) });
        if let Some(l) = &mut front.layouts {
            l.pressed_at(x0, y0);
        }
        window.dispatch_event(WindowEvent::PointerPressed { position: at(0.0), button: PointerEventButton::Left });
        for k in 1..=10 {
            window.dispatch_event(WindowEvent::PointerMoved { position: at(k as f32 / 10.0) });
            front.act_layouts();
            window.request_redraw();
            draw(&mut px);
        }
        if std::env::var_os("ECTOMATTER_DRAG_HOLD").is_none() {
            window.dispatch_event(WindowEvent::PointerReleased { position: at(1.0), button: PointerEventButton::Left });
            // (as the window does: the cursor asked for again the moment the press lets go)
            window.dispatch_event(WindowEvent::PointerMoved { position: at(1.0) });
        }
    }
    let mut rest_at = drag.get(2..4).map(|xy| LogicalPosition::new(xy[0], xy[1]));
    if let Ok(at) = std::env::var("ECTOMATTER_HOVER") {
        let mut xy = at.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        let position = LogicalPosition::new(xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0));
        window.dispatch_event(WindowEvent::PointerMoved { position });
        rest_at = Some(position);
    }
    front.act_layouts();
    // (as the window does: changed under a resting pointer, what is under it asked for again)
    if let (true, Some(position), None) = (front.layouts_rehover(), rest_at, std::env::var_os("ECTOMATTER_DRAG_HOLD")) {
        window.dispatch_event(WindowEvent::PointerMoved { position });
    }
    eprintln!("the cursor: {:?}", window.pointer());
    window.request_redraw();
    draw(&mut px);
    let mut pix = Pixmap::new(w, h).unwrap();
    for (d, p) in pix.pixels_mut().iter_mut().zip(&px) {
        *d = resvg::tiny_skia::PremultipliedColorU8::from_rgba(p.red, p.green, p.blue, p.alpha).unwrap_or(resvg::tiny_skia::PremultipliedColorU8::TRANSPARENT);
    }
    pix.save_png(path).unwrap();
    eprintln!("saved {path}: {w}x{h}");
    // ECTOMATTER_CLOSE: the window closed after the picture, which saves the layouts
    if std::env::var_os("ECTOMATTER_CLOSE").is_some() {
        front.layouts_closed();
    }
}

/// The lab's window without a window: one frame, saved as a PNG,
/// its readout from the keyboard lit as the snapshot's state has it.
fn snap_lab(mut front: Front, made: &Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, reference: f64, path: &str) {
    let (w, h) = std::env::var("ECTOMATTER_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or(LAB_SIZE);
    let mouse = std::env::var("ECTOMATTER_LAB").is_ok_and(|v| v == "mouse");
    // (the readout is the keyboard's board's, the mouse's lab too)
    let Some(core) = cores::keyboard() else {
        return eprintln!("ectomatter: there's no keyboard's core for the lab's snapshot");
    };
    let device = device_in_use(core, &front.settings);
    let board = settled(&core.tree(), &device, ink_env::rgb("stage", STAGE), 1264, 506);
    let ui = LabWindow::new().unwrap();
    let _ = slint::ComponentHandle::window(&ui);
    let window = made.borrow().last().cloned().unwrap();
    slint::ComponentHandle::show(&ui).unwrap();
    window.set_size(PhysicalSize::new(w, h));
    // ECTOMATTER_LAB=mouse: the lab on the mouse's lights (its picture drawn first)
    let mut lab = if mouse {
        front.ready_mouse_picture();
        let setup = setups::in_use(&front.settings, &mouse::shape().core).map(|s| s.id);
        lab::LabPage::new(ui, glyph(include_str!("../ui/glyphs/mark.svg"), 16, false), front.mouse_light_model(), mouse_lab_device(&front).expect("the mouse's picture"), setup)
    } else {
        let setup = setups::in_use(&front.settings, &core.id).map(|s| s.id);
        lab::LabPage::new(ui, glyph(include_str!("../ui/glyphs/mark.svg"), 16, false), device["light"].clone(), lab_device(core, Some(&board)), setup)
    };
    let lights = front.lights().unwrap_or_default();
    let gain = front.light_gain(reference);
    let readout = |board: &Board, front: &Front| {
        let p = board.peaks(&lights, reference as f32, gain);
        let (photons, settled) = board.sim_progress().unwrap_or((0, false));
        lab::Readout {
            legend: (p.legend as f64).round(),
            reach: (p.clean as f64).round(),
            deck: tenth(p.deck),
            skirt: tenth(p.skirt),
            chassis: tenth(p.chassis),
            lcd: (board.lcd_nits() as f64).min(gain as f64 * reference),
            peak: gain as f64 * reference,
            white: reference,
            photons,
            settled,
            lights: if mouse { front.mouse_brightest() } else { [0.0; 2] },
        }
    };
    lab.set_readout(readout(&board, &front));
    let rev = lab.rev;
    let mut px = vec![PremultipliedRgbaColor::default(); (w * h) as usize];
    let draw = |px: &mut Vec<PremultipliedRgbaColor>| {
        window.draw_if_needed(|r| {
            r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            r.render(px, w as usize);
        });
    };
    lab.refresh();
    window.request_redraw();
    draw(&mut px);
    for click in std::env::var("ECTOMATTER_CLICKS").unwrap_or_default().split(';').filter(|c| !c.is_empty()) {
        let mut xy = click.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        let position = LogicalPosition::new(xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0));
        window.dispatch_event(WindowEvent::PointerMoved { position });
        lab.pressed_at(position.x, position.y);
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        lab.act();
        // ECTOMATTER_IMPORT_FONT=file: the typeface chosen when the file picker opens
        if let (true, Some(file)) = (std::mem::take(&mut lab.typeface_wanted), std::env::var_os("ECTOMATTER_IMPORT_FONT")) {
            match typefaces::add(std::path::Path::new(&file)) {
                Ok(families) => lab.typeface_added(&families),
                Err(why) => lab.image_refused(&why),
            }
            lab.act();
        }
        // ECTOMATTER_IMPORT=file: the picture chosen when the file picker opens
        if let (Some(part), Some(file)) = (lab.image_wanted.take(), std::env::var_os("ECTOMATTER_IMPORT")) {
            match setups::import_image(&core.id, std::path::Path::new(&file)) {
                Ok(name) => lab.put_image(part, &name),
                Err(why) => lab.image_refused(&why),
            }
            lab.act();
        }
        window.request_redraw();
        draw(&mut px);
    }
    // ECTOMATTER_ESCAPE=N: Escape pressed N times after the clicks, as the window takes it
    for _ in 0..std::env::var("ECTOMATTER_ESCAPE").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0) {
        if !lab.typing() {
            lab.escape();
        }
        lab.act();
        window.request_redraw();
        draw(&mut px);
    }
    // ECTOMATTER_TYPE: keys typed into whatever has focus; a newline is
    // Return, a backspace (\u{8}) Backspace
    for ch in std::env::var("ECTOMATTER_TYPE").unwrap_or_default().chars() {
        let text: SharedString = if ch == '\n' { Key::Return.into() } else if ch == '\u{8}' { Key::Backspace.into() } else { ch.to_string().into() };
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        slint::platform::update_timers_and_animations();
        lab.act();
    }
    // ECTOMATTER_HOVER=x,y: the pointer moved there after the clicks
    if let Some((x, y)) = std::env::var("ECTOMATTER_HOVER").ok().and_then(|s| s.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)))) {
        window.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(x, y) });
        lab.act();
    }
    // ECTOMATTER_WAIT=ms: let that long pass with the pointer where the
    // clicks left it, for what shows once the pointer rests
    if let Some(ms) = std::env::var("ECTOMATTER_WAIT").ok().and_then(|v| v.parse::<u64>().ok()) {
        for _ in 0..clock::frames(ms) {
            clock::frame();
            slint::platform::update_timers_and_animations();
            lab.act();
        }
    }
    if let Some(dy) = std::env::var("ECTOMATTER_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
        let position = LogicalPosition::new(w as f32 / 2.0, h as f32 / 2.0);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: -dy });
        for _ in 0..clock::frames(400) {
            clock::frame();
            slint::platform::update_timers_and_animations();
        }
    }
    // ECTOMATTER_PRESS=x,y: the pointer pressed there and held, for what shows while it is
    if let Some((x, y)) = std::env::var("ECTOMATTER_PRESS").ok().and_then(|s| s.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)))) {
        let position = LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        lab.act();
    }
    lab.act();
    // the tuning changed: the readout as the window gives it once the device
    // has taken the tuning and its light has settled (the keyboard simulated
    // again with it, the mouse's lamps made again with it)
    if lab.rev != rev {
        let tuned = lab.shown().clone();
        let r = if mouse {
            front.set_mouse_light(tuned);
            readout(&board, &front)
        } else {
            let mut d = device.clone();
            d["light"] = tuned;
            readout(&settled(&core.tree(), &d, ink_env::rgb("stage", STAGE), 1264, 506), &front)
        };
        eprintln!("the lab's tuning changed: legend {:.0} nits, lights {:?} nits", r.legend, r.lights);
        lab.set_readout(r);
    }
    window.request_redraw();
    draw(&mut px);
    // ECTOMATTER_LIGHT=file: the light the lab has tuned, written there
    if let Some(out) = std::env::var_os("ECTOMATTER_LIGHT") {
        let _ = std::fs::write(out, serde_json::to_string_pretty(lab.shown()).unwrap_or_default());
    }
    let mut pix = Pixmap::new(w, h).unwrap();
    for (d, p) in pix.pixels_mut().iter_mut().zip(&px) {
        *d = resvg::tiny_skia::PremultipliedColorU8::from_rgba(p.red, p.green, p.blue, p.alpha).unwrap_or(resvg::tiny_skia::PremultipliedColorU8::TRANSPARENT);
    }
    pix.save_png(path).unwrap();
    eprintln!("saved {path}: {w}x{h}");
}

/// Lay the app's ground where the page leaves it to the window; true if it moved.
fn lay_ground(pic: &mut Picture, ui: &AppWindow, scale: f32) -> bool {
    let c = ui.get_ground();
    pic.set_ground(
        (ui.get_ground_x() * scale).round() as usize,
        (ui.get_ground_y() * scale).round() as usize,
        ui.get_ground_radius() * scale,
        [c.red(), c.green(), c.blue()],
    )
}

/// Where the keyboard's picture goes, in the buffer's pixels: in its page's
/// box, or on Home's desk where the desk has it (nothing there till Home's
/// keyboard is drawn, `home_drawn`).
fn keyboard_place(front: &Front, home_drawn: bool, s: f32) -> (i32, i32, usize, usize) {
    match front.desk_keyboard() {
        Some([x, y, w, h]) if home_drawn => ((x * s).round() as i32, (y * s).round() as i32, (w * s).round().max(1.0) as usize, (h * s).round().max(1.0) as usize),
        Some(_) => (0, 0, 0, 0),
        None => board_rect(&front.ui, s),
    }
}

/// The keyboard laid in the window's picture where keyboard_place puts it,
/// as `lit` shows it, and on Home's desk what the desk lays on the ground
/// round it and under it: how much of the ground shows through the keyboard
/// (`share`, its picture's) and how strongly the keyboard shows (dimmed while
/// it isn't plugged in). The window and a snapshot both lay it so. The boxes
/// to draw again.
fn lay_keyboard(pic: &mut Picture, front: &Front, home_drawn: bool, lit: Option<&Rc<Lit>>, share: Option<Rc<Vec<f32>>>, s: f32) -> Vec<(usize, usize, usize, usize)> {
    let rect = keyboard_place(front, home_drawn, s);
    let mut dirty = Vec::new();
    let before = pic.keyboard_rect();
    if pic.set_keyboard(rect, lit) {
        for r in [before, rect] {
            dirty.push((r.0.max(0) as usize, r.1.max(0) as usize, r.2, r.3));
        }
    }
    dirty.extend(pic.set_decor(front.desk_decor(s)));
    let (share, dim) = if front.desk_keyboard().is_some() { (share, if front.desk_keyboard_on() { 1.0 } else { 0.5 }) } else { (None, 1.0) };
    if pic.set_keyboard_ground(share, dim) && rect.2 > 0 {
        dirty.push((rect.0.max(0) as usize, rect.1.max(0) as usize, rect.2, rect.3));
    }
    dirty
}

/// The mouse laid in the window's picture in its page's hole, as its HDR
/// picture shows it now, or taken out where its page doesn't show it. The
/// window and a snapshot both lay it so. The boxes to draw again.
fn lay_mouse(pic: &mut Picture, front: &mut Front, s: f32) -> Vec<(usize, usize, usize, usize)> {
    let (rect, lit) = front.mouse_shown(s);
    let before = pic.mouse_rect();
    if !pic.set_mouse(rect, lit.as_ref()) {
        return Vec::new();
    }
    [before, rect].into_iter().filter(|r| r.2 > 0 && r.3 > 0).map(|r| (r.0.max(0) as usize, r.1.max(0) as usize, r.2, r.3)).collect()
}

/// Where the page's hole for the keyboard is, in the buffer's pixels.
fn board_rect(ui: &AppWindow, scale: f32) -> (i32, i32, usize, usize) {
    if !ui.get_board_shown() {
        return (0, 0, 0, 0);
    }
    (
        (ui.get_board_x() * scale).round() as i32,
        (ui.get_board_y() * scale).round() as i32,
        (ui.get_board_width() * scale).round().max(1.0) as usize,
        (ui.get_board_height() * scale).round().max(1.0) as usize,
    )
}

/// The device the lab tunes, as the shown core describes it: its
/// name, where its description is, which of the light's parts it has, and
/// (once its board is drawn) every cap, each in section as the art has it.
fn lab_device(core: &cores::Core, board: Option<&Board>) -> lab::LabDevice {
    let light = core.device_now()["light"].clone();
    let mm = sim::Physics::from(&light).units_per_mm;
    let typical = light["typical"].as_str().map(str::to_string).or_else(|| light["typical"].as_u64().map(|n| n.to_string()));
    lab::LabDevice {
        name: core.name().to_string(),
        kind: core.device["kind"].as_str().unwrap_or_default().to_string(),
        core: core.id.clone(),
        path: core.path.clone(),
        // (before its board is drawn, a device with art is taken to have keys and wells)
        keys: board.is_none_or(|b| b.cap_shapes.iter().any(|c| c.lit)),
        wells: board.is_none_or(|b| b.has_wells()),
        screens: core.device["screens"].as_array().is_some_and(|a| !a.is_empty()),
        caps: board.map(|b| b.cap_shapes.iter().map(|c| (c.clone(), b.key_geometry(&c.id, mm))).collect()).unwrap_or_default(),
        typical,
        print: core.device_now()["print"].clone(),
        windows: false,
        light_at: &["light"],
    }
}

/// The mouse as the lab tunes it: its lights, each a window of its own,
/// chosen one at a time where its window is on the drawing; its light kept
/// in its core's own file. None before its picture is drawn.
fn mouse_lab_device(front: &Front) -> Option<lab::LabDevice> {
    let parts = front.mouse_lab_parts()?;
    let desc = &front.mouse.state["device"];
    // (its light's file, as its core names it, in its core's folder)
    let shape = mouse::shape();
    let file = desc["light"].as_str()?;
    Some(lab::LabDevice {
        name: desc["name"].as_str().unwrap_or(&shape.core).to_string(),
        kind: desc["kind"].as_str().unwrap_or("mouse").to_string(),
        core: shape.core.clone(),
        path: shape.folder.join(file),
        keys: false,
        wells: false,
        screens: false,
        caps: parts.into_iter().map(|c| (c, None)).collect(),
        typical: None,
        print: serde_json::Value::Null,
        windows: true,
        light_at: &[],
    })
}

/// The keyboard lit: each part of its light at the core's brightness for it,
/// no channel past the ceiling, which is Peak nits over the desktop's white,
/// and its lock lights as the keyboard has them.
fn light(board: &Board, lights: &[Light], locks: u8, ceiling: f32, reference: f64, panel: Option<&overlays::Panel>) -> Vec<f32> {
    let mut px = Vec::new();
    light_into(&mut px, board, lights, locks, ceiling, reference, panel);
    px
}

/// The keyboard's picture as `light` draws it, into `px`, which an effect
/// keeps from one frame to the next rather than making it anew.
/// The keyboard dimmed and dulled everywhere but inside `rects`, so the keys
/// of one area stand out; the page round the keyboard (where `chassis`, its
/// coverage, is 0) stays as it is.
fn dim_around(px: &mut [f32], w: usize, rects: &[[f32; 4]], chassis: &[f32]) {
    let h = px.len() / 4 / w.max(1);
    let mut keep = vec![false; w * h];
    for r in rects {
        let (x0, y0) = (r[0].max(0.0) as usize, r[1].max(0.0) as usize);
        let (x1, y1) = (((r[0] + r[2]).ceil() as usize).min(w), ((r[1] + r[3]).ceil() as usize).min(h));
        for y in y0..y1 {
            keep[y * w + x0..y * w + x1.max(x0)].iter_mut().for_each(|k| *k = true);
        }
    }
    for (i, p) in px.chunks_exact_mut(4).enumerate() {
        if keep[i] {
            continue;
        }
        let a = chassis.get(i).copied().unwrap_or(1.0);
        if a <= 0.0 {
            continue;
        }
        let l = 0.2627 * p[0] + 0.678 * p[1] + 0.0593 * p[2];
        for c in &mut p[..3] {
            let dimmed = (l + (*c - l) * 0.6) * 0.38;
            *c += (dimmed - *c) * a;
        }
    }
}

fn light_into(px: &mut Vec<f32>, board: &Board, lights: &[Light], locks: u8, ceiling: f32, reference: f64, panel: Option<&overlays::Panel>) {
    board.compose_into(px, lights, locks, reference as f32, ceiling);
    // The display, lit from behind as an LCD is: its lit dots, its unlit dots
    // and the ground between them each at the core's nits for them, in the
    // backlight's color, under the glass. With nothing driving it, it is dark.
    if let Some(p) = panel {
        let (w, h) = (board.w as i32, board.h as i32);
        let lcd = board.lcd();
        let white = reference as f32;
        // the backlight is an LED: as much light as its color has, white the most
        let unit = p.backlight.map(|v| v.max(0.0).powf(2.2));
        let nits = |n: f32| (n / white).min(ceiling);
        let (lit, unlit, field) = (nits(lcd.lit), nits(lcd.unlit), nits(lcd.field));
        for row in 0..p.h as i32 {
            let yy = p.y + row;
            if yy < 0 || yy >= h {
                continue;
            }
            for col in 0..p.w as i32 {
                let x = p.x + col;
                if x < 0 || x >= w {
                    continue;
                }
                let i = (row * p.w as i32 + col) as usize;
                let e = if p.live {
                    let (on, off) = (p.on[i].min(1.0), p.off[i].min(1.0));
                    let k = on * lit + off * unlit + (1.0 - on - off).max(0.0) * field;
                    unit.map(|v| v * k)
                } else {
                    p.dark[i]
                };
                let d = ((yy * w + x) * 4) as usize;
                px[d..d + 3].copy_from_slice(&e);
                px[d + 3] = 1.0;
            }
        }
    }
}
