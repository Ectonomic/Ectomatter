//! Ectodeck as a native window: every pixel drawn by Ectodeck, no web view.
//!
//! The window is one HDR picture. The page (ui/app.slint) is drawn by Slint's
//! software renderer. The keyboard is drawn from the Z10 core's art by
//! ectodeck-draw and lit in linear light, brighter than the desktop's white
//! where it glows. The page leaves a hole where the keyboard goes; the two
//! meet in one picture, in linear light, which goes to the compositor as PQ
//! with BT.2020 primaries, 16 bits a channel. The page's colours are decoded
//! as KWin decodes every SDR app's (gamma 2.2 at the reference white), so the
//! page looks as the web app did and only the light goes above it.
//!
//! `ectodeck-native --snapshot FILE` draws one frame at 1440 x 940 without a
//! window and saves the picture as raw ABGR16161616, for checking. It reads
//! the keyboard's state from ECTODECK_NATIVE_STATE (a file the window saves
//! into ECTODECK_NATIVE_DUMP when it reads the keyboard), clicks where
//! ECTODECK_NATIVE_CLICKS says ("x,y;x,y", logical pixels) and types
//! ECTODECK_NATIVE_TYPE, so each state of the page can be checked offscreen.

mod adapter;
mod cores;
mod device;
mod display;
mod devices;
mod ecto_settings;
mod front;
mod kde_output;
mod lab;
mod lighting;
mod macro_page;
mod macros;
mod overlays;
mod picture;
mod screens;
mod settings;
mod setups;
mod text;
mod tray;
mod uistate;
mod z10;

slint::include_modules!();

use std::cell::RefCell;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ectodeck_draw::hdr::sim::{self, Scene, Setup, Simulator, Snapshot};
use ectodeck_draw::hdr::{Board, Light};
use front::{Front, WindowAsk};
use kde_output::client::kde_output_device_mode_v2::KdeOutputDeviceModeV2;
use kde_output::client::kde_output_device_registry_v2::{self, KdeOutputDeviceRegistryV2};
use kde_output::client::kde_output_device_v2::{self, KdeOutputDeviceV2};
use picture::{Lit, Picture};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use adapter::{Pointer, SoftWindow as MinimalSoftwareWindow};
use slint::platform::software_renderer::{PremultipliedRgbaColor, RepaintBufferType};
use slint::platform::{Key, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{LogicalPosition, PhysicalSize, SharedString};
use smithay_client_toolkit::reexports::calloop::{self, EventLoop};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use smithay_client_toolkit::reexports::client::{delegate_noop, Connection, Dispatch, QueueHandle};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell, XdgSurface,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use wayland_protocols::wp::color_management::v1::client::{
    wp_color_management_output_v1::{self, WpColorManagementOutputV1},
    wp_color_management_surface_v1::WpColorManagementSurfaceV1,
    wp_image_description_info_v1::{self, WpImageDescriptionInfoV1},
    wp_color_manager_v1::{self, WpColorManagerV1},
    wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1,
    wp_image_description_v1::{self, WpImageDescriptionV1},
};

/// the size the window opens at, and the least it may be, as the web app's
const SIZE: (u32, u32) = (1440, 940);
const MIN_SIZE: (u32, u32) = (1000, 700);
/// the Macro Editor's window, as the web app's
const MACRO_SIZE: (u32, u32) = (1080, 620);
const MACRO_MIN_SIZE: (u32, u32) = (900, 420);
/// the Key lab's
const LAB_SIZE: (u32, u32) = (1280, 800);
const LAB_MIN_SIZE: (u32, u32) = (1080, 680);
/// the windows of their own beside the main one, by their place in App's `subs`
const MACROS: usize = 0;
const LAB: usize = 1;
/// the ground the keyboard sits on: the page's own
const GROUND: [u8; 3] = [0x20, 0x20, 0x20];
/// How much brighter than the page's colours the keys' light is drawn. The
/// light is drawn at the web design's brightness, as approved in WebKit; going
/// above the desktop's white by the screen's headroom looked overdone.
const LIGHT_GAIN: f32 = 1.0;
/// how far in from the window's edge a press resizes it
const EDGE: f64 = 6.0;

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
        self.start.elapsed()
    }
}

/// A glyph from the app's drawings, as white on clear, for the page to tint.
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

/// Ectodeck's own folders, the web app's, so the two share settings and backups.
fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")).join("com.ectonomic.ectodeck")
}
fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")).join("com.ectonomic.ectodeck")
}

/// How bright the desktop's white is and how far the keyboard's light may go
/// above it: the Peak nits setting, or the brightest screen's peak.
fn levels(settings: &settings::Settings) -> (f64, f64) {
    let screens = screens::screens();
    let best = screens.iter().max_by(|a, b| a["peak"].as_f64().partial_cmp(&b["peak"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    // (the white the screen's user set, which an idle dim does not change)
    let reference = best.and_then(|s| s["white"].as_f64().or(s["reference"].as_f64())).filter(|r| *r > 0.0).unwrap_or(203.0);
    let peak = settings.get("hdr.glow_limit").and_then(|v| v.as_f64()).or_else(|| best.and_then(|s| s["peak"].as_f64())).unwrap_or(reference);
    (reference, peak.max(reference))
}

#[cfg(test)]
mod kept_tests {
    use super::*;
    use ectodeck_draw::hdr::sim::Physics;

    /// The device's keys to simulate, and a make of their light for each beam.
    fn scene() -> (Scene, impl Fn(f32) -> Setup) {
        let device: serde_json::Value = cores::shown().device_now();
        let tree = cores::shown().tree();
        let board = Board::new(&tree, &device, GROUND, 632, 253).unwrap();
        let base = Physics::from(&device["light"]);
        (Scene::of(&board), move |beam: f32| Setup::alike(Physics { beam, led: 7.0, ..base }))
    }

    /// A picture of another make than the one wanted is taken while the
    /// keyboard shows an older one (a slider dragged), but never over a
    /// picture of the one wanted.
    #[test]
    fn a_picture_on_the_way_is_taken_but_not_over_the_one_wanted() {
        let (scene, make) = scene();
        let (a, b, c) = (make(1.0), make(2.0), make(3.0));
        let pa = Simulator::settle(&scene, &a, 200);
        let pb = Simulator::settle(&scene, &b, 200);
        assert!(Kept::takes(Some(&c), Some(&pa), &pb), "on the way to the one wanted");
        assert!(Kept::takes(Some(&c), None, &pb), "on a keyboard with none");
        assert!(!Kept::takes(Some(&a), Some(&pa), &pb), "not over the one wanted");
        assert!(Kept::takes(Some(&a), Some(&pa), &pa), "the one wanted, clearer");
    }

    /// The last few settled pictures are kept, the newest found first; past
    /// the most kept the oldest goes, but never the saved tuning's.
    #[test]
    fn pictures_are_kept_but_the_saved_one_stays() {
        let (scene, make) = scene();
        let mut kept = Kept { pinned: Some(make(1.0)), ..Kept::default() };
        for beam in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            kept.keep(std::sync::Arc::new(Simulator::settle(&scene, &make(beam), 200)));
        }
        assert_eq!(kept.pictures.len(), Kept::MOST, "as many as are kept");
        assert!(kept.get(&make(1.0)).is_some(), "the saved tuning's stays");
        assert!(kept.get(&make(2.0)).is_none() && kept.get(&make(3.0)).is_none(), "the oldest others go");
        assert!(kept.get(&make(6.0)).is_some_and(|s| Kept::of(&s, &make(6.0))), "the newest found");
        // an LED output of its own is the same picture
        assert!(kept.get(&Setup::alike(Physics { lumens: 1.23, ..make(6.0).all })).is_some());
    }
}

/// Build the keyboard at each size asked for, away from the window's thread:
/// it takes a third of a second, and the window keeps drawing meanwhile.
fn board_builder(done: calloop::channel::Sender<Board>) -> std::sync::mpsc::Sender<(u32, u32)> {
    let (ask, asked) = std::sync::mpsc::channel::<(u32, u32)>();
    std::thread::spawn(move || {
        let tree = cores::shown().tree();
        let device: serde_json::Value = cores::shown().device_now();
        while let Ok(mut size) = asked.recv() {
            // only the latest size matters
            while let Ok(s) = asked.try_recv() {
                size = s;
            }
            let t = Instant::now();
            match Board::new(&tree, &device, GROUND, size.0, size.1) {
                Ok(b) => {
                    eprintln!("ectodeck-native: keyboard drawn at {}x{} in {:?}", size.0, size.1, t.elapsed());
                    if done.send(b).is_err() {
                        return;
                    }
                }
                Err(e) => eprintln!("ectodeck-native: the keyboard could not be drawn: {e}"),
            }
        }
    });
    ask
}

/// Set the legends a light asks for, away from the window's thread: setting
/// them takes a moment, and only the latest asked matters.
fn legends_builder(done: calloop::channel::Sender<ectodeck_draw::hdr::Legends>) -> std::sync::mpsc::Sender<ectodeck_draw::hdr::LegendsAsk> {
    let (ask, asked) = std::sync::mpsc::channel::<ectodeck_draw::hdr::LegendsAsk>();
    std::thread::spawn(move || {
        while let Ok(mut a) = asked.recv() {
            while let Ok(later) = asked.try_recv() {
                a = later;
            }
            let t = Instant::now();
            let legends = a.set(&ectodeck_draw::print::faces());
            eprintln!("ectodeck-native: legends set in {:?}", t.elapsed());
            if done.send(legends).is_err() {
                return;
            }
        }
    });
    ask
}

/// A brightness for the Key lab's readout: whole nits, or tenths below ten.
fn tenth(nits: f32) -> f64 {
    let n = nits as f64;
    if n < 10.0 { (n * 10.0).round() / 10.0 } else { n.round() }
}

/// The keyboard drawn at a size and lit by its light simulated to the end,
/// here and now: for a snapshot, which has no window to wait in.
fn settled(tree: &usvg::Tree, device: &serde_json::Value, w: u32, h: u32) -> Board {
    let mut board = Board::new(tree, device, GROUND, w, h).unwrap();
    let snap = Simulator::settle(&Scene::of(&board), &board.setup(&device["light"]), sim::SETTLED);
    board.set_sim(std::sync::Arc::new(snap));
    board
}

/// The core's device description, its light as the setup in use makes it
/// (the core's own, with none of the person's own in use).
fn device_in_use(settings: &settings::Settings) -> serde_json::Value {
    let core = cores::shown();
    let mut device = core.device_now();
    if let Some(s) = setups::in_use(settings, &core.id) {
        device["light"] = setups::merged(&device["light"], &s.changes);
    }
    device
}

/// A key as Slint takes it: its own text, or the name Slint gives a key with none.
fn key_text(e: &KeyEvent) -> Option<SharedString> {
    let k = match e.keysym {
        Keysym::BackSpace => Key::Backspace,
        Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::Return | Keysym::KP_Enter => Key::Return,
        Keysym::Escape => Key::Escape,
        Keysym::Delete | Keysym::KP_Delete => Key::Delete,
        Keysym::Left | Keysym::KP_Left => Key::LeftArrow,
        Keysym::Right | Keysym::KP_Right => Key::RightArrow,
        Keysym::Up | Keysym::KP_Up => Key::UpArrow,
        Keysym::Down | Keysym::KP_Down => Key::DownArrow,
        Keysym::Home | Keysym::KP_Home => Key::Home,
        Keysym::End | Keysym::KP_End => Key::End,
        Keysym::Page_Up => Key::PageUp,
        Keysym::Page_Down => Key::PageDown,
        Keysym::Shift_L | Keysym::Shift_R => Key::Shift,
        Keysym::Control_L | Keysym::Control_R => Key::Control,
        Keysym::Alt_L | Keysym::Alt_R => Key::Alt,
        Keysym::Super_L | Keysym::Super_R => Key::Meta,
        _ => {
            let t = e.utf8.clone().or_else(|| e.keysym.key_char().map(|c| c.to_string()))?;
            return (!t.is_empty() && !t.chars().any(char::is_control)).then(|| t.into());
        }
    };
    Some(k.into())
}

/// Ectodeck's own fonts for a keyboard's glyphs in the windows' text too
/// (the Key lab's glyph list, a key's text as typed): looked in after the
/// computer's own fonts, so a glyph the computer has no font for still shows.
fn glyph_fonts_in_windows() {
    use i_slint_core::textlayout::sharedparley::fontique::{Blob, GenericFamily};
    let _ = i_slint_core::context::with_global_context(
        || Err(slint::PlatformError::NoPlatform),
        |ctx| {
            let mut fonts = ctx.font_context().borrow_mut();
            let ours: Vec<_> = ectodeck_draw::print::GLYPH_FONTS.iter().flat_map(|&data| fonts.collection.register_fonts(Blob::new(std::sync::Arc::new(data)), None)).map(|(family, _)| family).collect();
            // the last of the families Slint looks in for a glyph, after the computer's own
            let computer: Vec<_> = fonts.collection.generic_families(GenericFamily::SystemUi).collect();
            fonts.collection.set_generic_families(GenericFamily::SystemUi, computer.into_iter().chain(ours));
        },
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let snapshot = args.iter().position(|a| a == "--snapshot").and_then(|i| args.get(i + 1)).cloned();
    // the typefaces legends can be set in, the core's own among them, loaded
    // in the background so the Key lab's list of them opens at once
    ectodeck_draw::print::add_fonts(cores::shown().fonts());
    ectodeck_draw::print::warm();
    // `--read-state FILE`: read the keyboard's state once, for the snapshots, and stop
    if let Some(path) = args.iter().position(|a| a == "--read-state").and_then(|i| args.get(i + 1)) {
        let kb = ectotype_core::Keyboard::new(ectotype_core::Conn::hid(), None);
        let state = kb.state();
        kb.release();
        match state {
            Ok(s) => std::fs::write(path, serde_json::to_vec(&s).unwrap()).unwrap(),
            Err(e) => eprintln!("ectodeck-native: the keyboard could not be read: {e}"),
        }
        return;
    }

    let made = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(Platform { made: made.clone(), start: Instant::now() })).unwrap();
    glyph_fonts_in_windows();
    let ui = AppWindow::new().unwrap();
    let _ = slint::ComponentHandle::window(&ui);
    let window = made.borrow().last().cloned().expect("the main window's adapter");
    ui.set_mark(glyph(include_str!("../ui/glyphs/mark.svg"), 16, false));
    ui.set_home_glyph(glyph(include_str!("../ui/glyphs/home.svg"), 22, true));
    ui.set_keyboard_glyph(glyph(include_str!("../ui/glyphs/keyboard.svg"), 22, true));
    ui.set_settings_glyph(glyph(include_str!("../ui/glyphs/settings.svg"), 22, true));
    ui.set_keyboard_tile(glyph(include_str!("../ui/glyphs/keyboard.svg"), 88, true));
    ui.set_rail_open(uistate::get("rail_open").and_then(|v| v.as_bool()).unwrap_or(false));
    slint::ComponentHandle::show(&ui).unwrap();

    let settings = settings::Settings::load(config_dir().join("settings.json"));
    let (reference, glow) = levels(&settings);
    let gain = LIGHT_GAIN;
    eprintln!("ectodeck-native: the desktop's white is {reference} nits; the light goes to {glow}");

    if let Some(path) = snapshot {
        return snap(&window, &made, ui, reference, gain, &path);
    }
    // back on the place Ectodeck was started again from, for a change to what it may use
    if let Some(p) = args.iter().position(|a| a == "--place").and_then(|i| args.get(i + 1)) {
        ui.set_place(p.as_str().into());
    }
    run(window, made, ui, settings, reference, gain);
}

/// One frame without a window, saved for checking.
fn snap(window: &Rc<MinimalSoftwareWindow>, made: &Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, ui: AppWindow, reference: f64, gain: f32, path: &str) {
    let (mut w, mut h) = std::env::var("ECTODECK_NATIVE_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or(SIZE);
    window.set_size(PhysicalSize::new(w, h));
    let mut front = Front::new(ui, None, true, settings::Settings::load(config_dir().join("settings.json")));
    if let Some(state) = std::env::var_os("ECTODECK_NATIVE_STATE") {
        let dev: serde_json::Value = serde_json::from_slice(&std::fs::read(state).expect("the saved state")).expect("the saved state as JSON");
        front.heard(device::Heard::State(Ok(dev)));
    }
    if let Ok(place) = std::env::var("ECTODECK_NATIVE_PLACE") {
        front.ui.set_place(place.into());
    }
    // ECTODECK_NATIVE_MACROS=N: the Macro Editor's window instead, open on macro N
    if let Some(n) = std::env::var("ECTODECK_NATIVE_MACROS").ok().and_then(|v| v.parse::<u32>().ok()) {
        return snap_macros(front, made, n, path);
    }
    // ECTODECK_NATIVE_LAB: the Key lab's window instead
    if std::env::var_os("ECTODECK_NATIVE_LAB").is_some() {
        return snap_lab(front, made, reference, path);
    }
    // lay the page out once, so clicks land where it draws
    let mut pic = Picture::new(reference, GROUND);
    pic.resize(w as usize, h as usize);
    window.draw_if_needed(|r| {
        r.render(&mut pic.ui, w as usize);
    });
    for click in std::env::var("ECTODECK_NATIVE_CLICKS").unwrap_or_default().split(';').filter(|c| !c.is_empty()) {
        let mut xy = click.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        let (x, y) = (xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0));
        let position = LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        front.pressed_at(x, y);
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        // ECTODECK_NATIVE_MIDCLICK: the page refreshed between the press and the
        // release, as a reading from the keyboard does ten times a second
        if std::env::var_os("ECTODECK_NATIVE_MIDCLICK").is_some() {
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
    for c in std::env::var("ECTODECK_NATIVE_TYPE").unwrap_or_default().chars() {
        let text: SharedString = c.to_string().into();
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        slint::platform::update_timers_and_animations();
        front.act();
    }
    // the wheel turned over the page, in logical pixels
    if let Some(dy) = std::env::var("ECTODECK_NATIVE_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
        // where the wheel turns: ECTODECK_NATIVE_SCROLL_AT="x,y", or the page's middle
        let at: Vec<f32> = std::env::var("ECTODECK_NATIVE_SCROLL_AT").unwrap_or_default().split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let position = if let [x, y] = at[..] { LogicalPosition::new(x, y) } else { LogicalPosition::new(700.0, 600.0) };
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: -dy });
        let end = Instant::now() + Duration::from_millis(400);
        while Instant::now() < end {
            std::thread::sleep(Duration::from_millis(16));
            slint::platform::update_timers_and_animations();
        }
    }
    // keys typed on the keyboard, as Linux key codes: "29+" down, "29-" up
    // keys as the keyboard sends them: into the window, and as its own devices tell of them
    for k in std::env::var("ECTODECK_NATIVE_EVDEV").unwrap_or_default().split(',').filter(|k| !k.is_empty()) {
        let (code, down) = (k.trim_end_matches(['+', '-']).parse().unwrap_or(0), k.ends_with('+'));
        front.typed(code, down);
        front.watched(ectotype_core::watch::Watched::Key(code as u16, down));
        front.refresh();
    }
    // ECTODECK_NATIVE_SLIDERS='{"dimmer":64,...}': a reading of the sliders, as the keyboard sends one
    if let Some(r) = std::env::var("ECTODECK_NATIVE_SLIDERS").ok().and_then(|v| serde_json::from_str(&v).ok()) {
        front.heard(device::Heard::Sliders(Ok(r)));
    }
    // ECTODECK_NATIVE_ACTIVE=N: the keyboard reports it is using profile N, as when it is switched on the keyboard
    if let Some(n) = std::env::var("ECTODECK_NATIVE_ACTIVE").ok().and_then(|v| v.parse::<u8>().ok()) {
        front.heard(device::Heard::ActiveNow(Ok(Some(n))));
    }
    // ECTODECK_NATIVE_LOCKS=N: the keyboard's lock lights (1 Num Lock, 2 Caps Lock, 4 Scroll Lock)
    if let Some(bits) = std::env::var("ECTODECK_NATIVE_LOCKS").ok().and_then(|v| v.parse::<u8>().ok()) {
        front.watched(ectotype_core::watch::Watched::Locks(bits));
    }
    // the pointer, where it was last left
    if let Ok(at) = std::env::var("ECTODECK_NATIVE_HOVER") {
        let mut xy = at.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        window.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0)) });
    }
    // ECTODECK_NATIVE_DRAG="x,y;x,y": a press dragged from the first point to the
    // second in 30 steps, each step timed as the window would handle it
    if let Ok(d) = std::env::var("ECTODECK_NATIVE_DRAG") {
        let pts: Vec<(f32, f32)> = d
            .split(';')
            .filter_map(|p| p.split_once(',').and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?))))
            .collect();
        if let [(x0, y0), (x1, y1)] = pts[..] {
            let at = |t: f32| LogicalPosition::new(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            window.dispatch_event(WindowEvent::PointerMoved { position: at(0.0) });
            window.dispatch_event(WindowEvent::PointerPressed { position: at(0.0), button: PointerEventButton::Left });
            let rect = board_rect(&front.ui, 1.0);
            let tree = cores::shown().tree();
            let device = device_in_use(&front.settings);
            let board = (rect.2 > 0).then(|| settled(&tree, &device, rect.2 as u32, rect.3 as u32));
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
            window.dispatch_event(WindowEvent::PointerReleased { position: at(1.0), button: PointerEventButton::Left });
            front.act();
        }
    }
    // let animations run their course, as the eye would see them settle
    if let Some(ms) = std::env::var("ECTODECK_NATIVE_WAIT").ok().and_then(|v| v.parse::<u64>().ok()) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            std::thread::sleep(Duration::from_millis(16));
            slint::platform::update_timers_and_animations();
            front.act();
        }
    }
    eprintln!("the cursor: {:?}", window.pointer());
    // (the caps' colours, for what is printed over the keys)
    let in_use = device_in_use(&front.settings)["light"].clone();
    front.light_caps(&in_use);
    front.refresh();
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
    if rect.2 > 0 {
        let tree = cores::shown().tree();
        let device = device_in_use(&front.settings);
        let t = Instant::now();
        let board = settled(&tree, &device, rect.2 as u32, rect.3 as u32);
        eprintln!("keyboard drawn and its light simulated in {:?}", t.elapsed());
        let lights = front.lights().unwrap_or_default();
        let gain = front.light_gain(reference).max(gain);
        eprintln!("the light at {:.0} nits", gain as f64 * reference);
        let lit = light(&board, &lights, front.page.locks, gain, reference, front.overlays.panel_for(board.w, board.lcd().dot));
        let lit = Rc::new(Lit { px: lit, w: rect.2, h: rect.3 });
        pic.set_keyboard(rect, Some(&lit));
        front.ui.set_board_ready(true);
        window.request_redraw();
        window.draw_if_needed(|r| {
            r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            r.render(&mut pic.ui, w as usize);
        });
        // ECTODECK_NATIVE_RESIZE=WxH: the window made that size as the window does
        // mid-resize: the page laid out and drawn anew, the keyboard's last
        // picture stretched to its new place until a new one is drawn
        if let Some((nw, nh)) = std::env::var("ECTODECK_NATIVE_RESIZE").ok().and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))) {
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
            pic.set_keyboard(r2, Some(&lit));
            eprintln!("resized to {nw}x{nh}: keyboard now at {r2:?}, its picture still {}x{}", lit.w, lit.h);
            if std::env::var_os("ECTODECK_NATIVE_DEBUG").is_some() {
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
    let t = Instant::now();
    pic.encode_all();
    eprintln!("frame encoded in {:?}", t.elapsed());
    std::fs::write(path, bytemuck::cast_slice::<[u16; 4], u8>(&pic.out)).unwrap();
    // the white it was encoded for, which decoding it needs
    std::fs::write(format!("{path}.reference"), format!("{reference}\n")).unwrap();
    eprintln!("saved {path}: {w}x{h}, keyboard at {rect:?}");
}

/// The Macro Editor's window without a window: one frame, saved as a PNG.
fn snap_macros(mut front: Front, made: &Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, n: u32, path: &str) {
    let (w, h) = std::env::var("ECTODECK_NATIVE_SIZE")
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
    if let Some(m) = &mut front.macros {
        m.open(&front.page, (n > 0).then_some(n));
    }
    window.request_redraw();
    draw(&mut px);
    for click in std::env::var("ECTODECK_NATIVE_CLICKS").unwrap_or_default().split(';').filter(|c| !c.is_empty()) {
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
    for c in std::env::var("ECTODECK_NATIVE_TYPE").unwrap_or_default().chars() {
        let text: SharedString = if c == '\n' { Key::Return.into() } else { c.to_string().into() };
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        slint::platform::update_timers_and_animations();
        front.act_macros();
    }
    // ECTODECK_NATIVE_DRAG="x,y;x,y": a press at the first point, moved to the
    // second; let go there only with ECTODECK_NATIVE_DRAG_RELEASE
    if let Ok(d) = std::env::var("ECTODECK_NATIVE_DRAG") {
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
            if std::env::var_os("ECTODECK_NATIVE_DRAG_RELEASE").is_some() {
                window.dispatch_event(WindowEvent::PointerReleased { position: at(1.0), button: PointerEventButton::Left });
            }
        }
    }
    if let Ok(at) = std::env::var("ECTODECK_NATIVE_HOVER") {
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

/// The Key lab's window without a window: one frame, saved as a PNG,
/// its readout from the keyboard lit as the snapshot's state has it.
fn snap_lab(mut front: Front, made: &Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, reference: f64, path: &str) {
    let (w, h) = std::env::var("ECTODECK_NATIVE_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or(LAB_SIZE);
    let device = device_in_use(&front.settings);
    let tree = cores::shown().tree();
    let board = settled(&tree, &device, 1264, 506);
    let ui = LabWindow::new().unwrap();
    let _ = slint::ComponentHandle::window(&ui);
    let window = made.borrow().last().cloned().unwrap();
    slint::ComponentHandle::show(&ui).unwrap();
    window.set_size(PhysicalSize::new(w, h));
    let setup = setups::in_use(&front.settings, &cores::shown().id).map(|s| s.id);
    let mut lab = lab::LabPage::new(ui, glyph(include_str!("../ui/glyphs/mark.svg"), 16, false), device["light"].clone(), lab_device(Some(&board)), setup);
    let lights = front.lights().unwrap_or_default();
    let gain = front.light_gain(reference);
    let p = board.peaks(&lights, reference as f32, gain);
    let (photons, settled) = board.sim_progress().unwrap_or((0, false));
    lab.set_readout(lab::Readout {
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
    });
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
    for click in std::env::var("ECTODECK_NATIVE_CLICKS").unwrap_or_default().split(';').filter(|c| !c.is_empty()) {
        let mut xy = click.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        let position = LogicalPosition::new(xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0));
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        window.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        lab.act();
        window.request_redraw();
        draw(&mut px);
    }
    // ECTODECK_NATIVE_TYPE: keys typed into whatever has focus; a newline is
    // Return, a backspace (\u{8}) Backspace
    for ch in std::env::var("ECTODECK_NATIVE_TYPE").unwrap_or_default().chars() {
        let text: SharedString = if ch == '\n' { Key::Return.into() } else if ch == '\u{8}' { Key::Backspace.into() } else { ch.to_string().into() };
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        slint::platform::update_timers_and_animations();
        lab.act();
    }
    // ECTODECK_NATIVE_HOVER=x,y: the pointer moved there after the clicks
    if let Some((x, y)) = std::env::var("ECTODECK_NATIVE_HOVER").ok().and_then(|s| s.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)))) {
        window.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(x, y) });
        lab.act();
    }
    // ECTODECK_NATIVE_WAIT=ms: let that long pass with the pointer where the
    // clicks left it, for what shows once the pointer rests
    if let Some(ms) = std::env::var("ECTODECK_NATIVE_WAIT").ok().and_then(|v| v.parse::<u64>().ok()) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            std::thread::sleep(Duration::from_millis(16));
            slint::platform::update_timers_and_animations();
            lab.act();
        }
    }
    if let Some(dy) = std::env::var("ECTODECK_NATIVE_SCROLL").ok().and_then(|v| v.parse::<f32>().ok()) {
        let position = LogicalPosition::new(w as f32 / 2.0, h as f32 / 2.0);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: -dy });
        let end = Instant::now() + Duration::from_millis(400);
        while Instant::now() < end {
            std::thread::sleep(Duration::from_millis(16));
            slint::platform::update_timers_and_animations();
        }
    }
    // ECTODECK_NATIVE_PRESS=x,y: the pointer pressed there and held, for what shows while it is
    if let Some((x, y)) = std::env::var("ECTODECK_NATIVE_PRESS").ok().and_then(|s| s.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)))) {
        let position = LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        lab.act();
    }
    lab.act();
    window.request_redraw();
    draw(&mut px);
    // ECTODECK_NATIVE_LIGHT=file: the light the lab has tuned, written there
    if let Some(out) = std::env::var_os("ECTODECK_NATIVE_LIGHT") {
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

struct App {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    _xdg: XdgShell,
    conn: Connection,
    qh: QueueHandle<App>,
    window: Window,
    pool: SlotPool,
    buffers: Vec<(Buffer, Vec<Rect>)>,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    seat: Option<wl_seat::WlSeat>,
    cursor: Option<CursorIcon>,
    /// the window's size in logical pixels, and the buffer's scale
    size: (u32, u32),
    scale: u32,
    configured: bool,
    maximized: bool,
    frame_pending: bool,
    exit: bool,
    /// the last button press, which a move or a resize is started from
    press: u32,
    at: (f64, f64),
    ready: bool,
    failed: Option<String>,
    /// colour management: the picture's description on the surface, each
    /// screen's levels as KWin describes them, the screen the window is on,
    /// and the levels the picture is drawn for (the desktop's white, the light's top)
    cm: WpColorManagerV1,
    cm_surface: WpColorManagementSurfaceV1,
    screens: Vec<ScreenLevels>,
    /// what KDE says of each screen (its set SDR white), as its last word
    /// left it and as its words come in
    kde: Vec<(KdeOutputDeviceV2, kde_output::Device, kde_output::Device)>,
    _kde_registry: Option<KdeOutputDeviceRegistryV2>,
    on_screen: Option<u32>,
    glow_limit: Option<f64>,
    levels: (f64, f64),
    /// the screen's own peak, for Settings › HDR
    levels_peak: f64,
    /// KWin takes the mastering display's luminance in a description
    mastering: bool,
    /// the display's drawing the keyboard was last lit with
    panel_rev: u64,

    slint: Rc<MinimalSoftwareWindow>,
    front: Front,
    pic: Picture,
    gain: f32,
    /// the keyboard at the size last drawn, and its light
    board: Option<Board>,
    lights: Vec<Light>,
    lit: Option<Rc<Lit>>,
    /// the last frame's keyboard picture, taken back once nothing shows it,
    /// for the next to be drawn into rather than made anew
    spare: Vec<f32>,
    /// when a lighting effect's next frame is due, at the rate Settings ›
    /// Performance asks for
    effect_due: Instant,
    asked: Option<(u32, u32)>,
    build: std::sync::mpsc::Sender<(u32, u32)>,
    /// legends to set as the light asks, away from the window
    set_legends: std::sync::mpsc::Sender<ectodeck_draw::hdr::LegendsAsk>,
    /// the whole window needs drawing again, not only what Slint changed
    full: bool,
    /// ECTODECK_NATIVE_DUMP: a folder to save the frames given to the compositor in
    dump: Option<PathBuf>,
    /// ECTODECK_NATIVE_CHECK: encode every frame whole too, and say if the
    /// parts encoded differ from it
    check: bool,
    frames: u64,
    /// closed into the tray: the window is unmapped, Ectodeck keeps running
    hidden: bool,
    tray: Option<tray::Handle>,
    tray_connected: bool,
    /// the windows Slint has made, the Macro Editor's among them
    made: Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>,
    /// the windows of their own beside the main one (the Macro Editor's, the
    /// Key lab's), once opened, and which of them has the keyboard's focus
    subs: [Option<SubWin>; 2],
    keys_on: Option<usize>,
    /// the keys held down as the keyboard last said, for Ctrl+Shift+L
    mods: Modifiers,
    /// the light the Key lab has tuned, which the keyboard is lit by, and
    /// its change (and whether Hold to compare was held for it)
    light: Option<serde_json::Value>,
    light_rev: (u64, bool),
    readout_at: Option<Instant>,
    /// the keys' light, simulated from the first keyboard drawn on, its way
    /// of saying it has a clearer picture, and the picture last taken
    sim: Option<Simulator>,
    sim_tx: calloop::channel::Sender<()>,
    sim_seen: u64,
    /// the simulation the keyboard is to show (how every key is lit), and
    /// the last few settled pictures, the saved tuning's kept among them
    want: Option<Setup>,
    kept: Kept,
}

/// Settled pictures of the keys' light, the last few kept, so going back to
/// one (an undo, Hold to compare, a choice taken back) shows at once rather
/// than settling again: each is about 27 MB. The saved tuning's stays.
#[derive(Default)]
struct Kept {
    pictures: Vec<std::sync::Arc<Snapshot>>,
    pinned: Option<Setup>,
}

impl Kept {
    const MOST: usize = 4;

    /// Whether a picture is of this setup, as the simulation runs it.
    fn of(snap: &Snapshot, setup: &Setup) -> bool {
        snap.setup.same_paths(&setup.simulated())
    }

    fn get(&self, want: &Setup) -> Option<std::sync::Arc<Snapshot>> {
        self.pictures.iter().rev().find(|s| Kept::of(s, want)).cloned()
    }

    /// Whether the keyboard takes the simulation's new picture. The
    /// simulation may still be on a make asked of it before the one wanted
    /// (a slider dragged): its picture is newer than the one shown, so it is
    /// taken, but never over a picture of the one wanted.
    fn takes(want: Option<&Setup>, shown: Option<&Snapshot>, snap: &Snapshot) -> bool {
        want.is_none_or(|w| Kept::of(snap, w) || !shown.is_some_and(|s| Kept::of(s, w)))
    }

    /// Keep a settled picture, the newest last; past the most kept, the oldest
    /// goes, but never the saved tuning's.
    fn keep(&mut self, snap: std::sync::Arc<Snapshot>) {
        self.pictures.retain(|s| !s.setup.same_paths(&snap.setup));
        self.pictures.push(snap);
        while self.pictures.len() > Kept::MOST {
            let pinned = &self.pinned;
            let Some(i) = self.pictures.iter().position(|s| !pinned.as_ref().is_some_and(|w| Kept::of(s, w))) else { break };
            self.pictures.remove(i);
        }
    }
}

/// A window of its own beside the main one: an ordinary SDR window, drawn whole by Slint.
struct SubWin {
    window: Window,
    slint: Rc<MinimalSoftwareWindow>,
    size: (u32, u32),
    scale: u32,
    configured: bool,
    maximized: bool,
    frame_pending: bool,
    hidden: bool,
    full: bool,
    buffers: Vec<(Buffer, Vec<Rect>)>,
    px: Vec<PremultipliedRgbaColor>,
    at: (f64, f64),
}

/// A screen's levels as KWin describes them: its white and its peak, in nits.
struct ScreenLevels {
    id: u32,
    cmo: WpColorManagementOutputV1,
    reference: f64,
    peak: f64,
    reading: (f64, f64),
    /// the white it reported last while not dimmed (see kde_output::white)
    undimmed: Option<f64>,
}

/// Which screen a description or its information was asked for.
#[derive(Clone, Copy)]
struct ScreenTag(u32);
/// A new description for the picture, for these levels (white, light's top).
#[derive(Clone, Copy)]
struct PictureTag(f64, f64);

/// The picture's description: PQ with BT.2020 primaries, its white at the
/// desktop's, and its brightest light no brighter than the screen's peak.
/// Without that last, KWin takes the picture to reach PQ's 10,000 nits and
/// tone-maps it down to the screen, which dims every light well below the
/// nits it was drawn at.
fn describe<U: Send + Sync + 'static>(cm: &WpColorManagerV1, qh: &QueueHandle<App>, reference: f64, peak: f64, mastering: bool, tag: U) -> WpImageDescriptionV1
where
    App: Dispatch<WpImageDescriptionV1, U>,
{
    let params = cm.create_parametric_creator(qh, ());
    params.set_tf_named(wp_color_manager_v1::TransferFunction::St2084Pq);
    params.set_primaries_named(wp_color_manager_v1::Primaries::Bt2020);
    params.set_luminances(0, 10000, reference.round() as u32);
    let peak = peak.max(reference + 1.0).round() as u32;
    if mastering {
        params.set_mastering_luminance(1, peak);
    }
    params.set_max_cll(peak);
    params.set_max_fall(peak);
    params.create(qh, tag)
}

fn run(slint: Rc<MinimalSoftwareWindow>, made: Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, ui: AppWindow, settings: settings::Settings, reference: f64, gain: f32) {
    let conn = Connection::connect_to_env().expect("a Wayland session");
    let (globals, queue) = registry_queue_init::<App>(&conn).unwrap();
    let qh = queue.handle();
    let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap();
    WaylandSource::new(conn.clone(), queue).insert(event_loop.handle()).unwrap();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let xdg = XdgShell::bind(&globals, &qh).expect("xdg_shell");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let cm: WpColorManagerV1 = globals.bind(&qh, 1..=1, ()).expect("the compositor's colour management (KWin 6.3 or later)");
    // KDE's account of each screen, for the SDR white its user set (none on another compositor)
    let kde_registry: Option<KdeOutputDeviceRegistryV2> = globals.bind(&qh, 21..=23, ()).ok();

    let surface = compositor.create_surface(&qh);
    let window = xdg.create_window(surface, WindowDecorations::RequestClient, &qh);
    window.set_title("Ectodeck");
    window.set_app_id("com.ectonomic.ectodeck");
    window.set_min_size(Some(MIN_SIZE));

    let desc = describe(&cm, &qh, reference, screens::screens().iter().filter_map(|s| s["peak"].as_f64()).fold(reference, f64::max), false, ());
    let cm_surface = cm.get_surface(window.wl_surface(), &qh, ());

    let (built_tx, built_rx) = calloop::channel::channel::<Board>();
    let build = board_builder(built_tx);
    // the legends, set anew as the Key lab asks
    let (legends_tx, legends_rx) = calloop::channel::channel::<ectodeck_draw::hdr::Legends>();
    let set_legends = legends_builder(legends_tx);
    event_loop
        .handle()
        .insert_source(legends_rx, |e, _, app: &mut App| {
            if let calloop::channel::Event::Msg(legends) = e {
                app.legends_ready(legends);
            }
        })
        .unwrap();
    event_loop
        .handle()
        .insert_source(built_rx, |e, _, app: &mut App| {
            if let calloop::channel::Event::Msg(board) = e {
                app.built(board);
            }
        })
        .unwrap();
    // each clearer picture of the keys' light, as the simulation has one
    let (sim_tx, sim_rx) = calloop::channel::channel::<()>();
    event_loop
        .handle()
        .insert_source(sim_rx, |e, _, app: &mut App| {
            if let calloop::channel::Event::Msg(()) = e {
                app.sim_ready();
            }
        })
        .unwrap();

    // the keyboard, unless Ectodeck is set to leave it to other programs
    let controlled = settings.on("devices.z10.control");
    let kb = controlled.then(|| {
        let (heard_tx, heard_rx) = calloop::channel::channel::<device::Heard>();
        event_loop
            .handle()
            .insert_source(heard_rx, |e, _, app: &mut App| {
                if let calloop::channel::Event::Msg(h) = e {
                    app.front.heard(h);
                }
            })
            .unwrap();
        // the keyboard's keys and lock lights, from its own devices, whichever window has it
        let (watch_tx, watch_rx) = calloop::channel::channel::<ectotype_core::watch::Watched>();
        event_loop
            .handle()
            .insert_source(watch_rx, |e, _, app: &mut App| {
                if let calloop::channel::Event::Msg(w) = e {
                    app.front.watched(w);
                }
            })
            .unwrap();
        ectotype_core::watch::start(move |w| {
            let _ = watch_tx.send(w);
        });
        device::start(data_dir().join("backups"), heard_tx)
    });

    // the keyboard lit by the light setup in use, if it is one of the person's own
    let lit_by = setups::in_use(&settings, &cores::shown().id).map(|s| setups::merged(&cores::shown().device_now()["light"], &s.changes));
    let mut app = App {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        pool: SlotPool::new((SIZE.0 * SIZE.1 * 8) as usize, &shm).expect("a shared memory pool"),
        compositor,
        shm,
        _xdg: xdg,
        conn: conn.clone(),
        qh: qh.clone(),
        window,
        buffers: Vec::new(),
        pointer: None,
        keyboard: None,
        seat: None,
        cursor: None,
        size: SIZE,
        scale: 1,
        configured: false,
        maximized: false,
        frame_pending: false,
        exit: false,
        press: 0,
        at: (0.0, 0.0),
        ready: false,
        failed: None,
        cm,
        cm_surface: cm_surface.clone(),
        screens: Vec::new(),
        kde: Vec::new(),
        _kde_registry: kde_registry,
        on_screen: None,
        glow_limit: settings.get("hdr.glow_limit").and_then(|v| v.as_f64()),
        levels_peak: 1000.0,
        mastering: false,
        panel_rev: 0,
        levels: (reference, reference * gain as f64),
        slint,
        front: Front::new(ui, kb, controlled, settings),
        pic: Picture::new(reference, GROUND),
        gain,
        board: None,
        lights: Vec::new(),
        lit: None,
        spare: Vec::new(),
        effect_due: Instant::now(),
        asked: None,
        build,
        set_legends,
        full: true,
        dump: std::env::var_os("ECTODECK_NATIVE_DUMP").map(Into::into),
        check: std::env::var_os("ECTODECK_NATIVE_CHECK").is_some(),
        frames: 0,
        hidden: false,
        tray: None,
        tray_connected: false,
        made,
        subs: [None, None],
        keys_on: None,
        mods: Modifiers::default(),
        light: lit_by,
        light_rev: (0, false),
        readout_at: None,
        sim: None,
        sim_tx,
        sim_seen: 0,
        want: None,
        kept: Kept::default(),
    };
    // the tray, which the window closes into
    let (tray_tx, tray_rx) = calloop::channel::channel::<tray::TrayAsk>();
    app.tray = tray::start(tray_tx);
    event_loop
        .handle()
        .insert_source(tray_rx, |e, _, app: &mut App| {
            if let calloop::channel::Event::Msg(a) = e {
                match a {
                    tray::TrayAsk::Open => app.show(),
                    tray::TrayAsk::Device(d) => {
                        app.show();
                        app.front.ui.set_place(d.into());
                    }
                    tray::TrayAsk::Quit => {
                        app.front.stop_preview();
                        app.exit = true;
                    }
                }
            }
        })
        .unwrap();

    // the picture's description has to be ready before the surface takes it
    while !app.ready && app.failed.is_none() {
        event_loop.dispatch(Some(Duration::from_millis(50)), &mut app).unwrap();
    }
    if let Some(e) = &app.failed {
        eprintln!("ectodeck-native: the compositor refused the HDR picture: {e}");
        return;
    }
    cm_surface.set_image_description(&desc, wp_color_manager_v1::RenderIntent::Perceptual);
    eprintln!("ectodeck-native: the compositor {} the mastering display's luminance", if app.mastering { "takes" } else { "does not take" });
    // mapped by a first commit with no buffer; the compositor answers with the size
    app.window.commit();

    let mut trial = std::env::var_os("ECTODECK_NATIVE_TRAYTEST").map(|_| Instant::now());
    let mut fps = std::env::var_os("ECTODECK_NATIVE_FPS").map(|_| (Instant::now(), 0u32, Duration::ZERO));
    // ECTODECK_NATIVE_OPEN_MACROS=N: open the Macro Editor on macro N once the keyboard is read, to check it
    let mut open_macros = std::env::var("ECTODECK_NATIVE_OPEN_MACROS").ok().and_then(|v| v.parse::<u32>().ok());
    // ECTODECK_NATIVE_OPEN_LAB: open the Key lab at start, as Ctrl+Shift+L does
    let mut open_lab = std::env::var_os("ECTODECK_NATIVE_OPEN_LAB").is_some();
    while !app.exit {
        let busy = (app.slint.has_active_animations() && !app.frame_pending)
            || app.subs.iter().flatten().any(|m| !m.hidden && m.slint.has_active_animations() && !m.frame_pending);
        // a lighting effect's next frame, at the rate Settings › Performance asks for
        let effect = (app.front.animated() && !app.frame_pending).then(|| app.effect_due.saturating_duration_since(Instant::now()));
        let wait = if busy {
            Some(Duration::ZERO)
        } else {
            [slint::platform::duration_until_next_timer_update(), app.front.waiting(), effect].into_iter().flatten().min()
        };
        event_loop.dispatch(wait, &mut app).unwrap();
        slint::platform::update_timers_and_animations();
        app.front.retry();
        // ECTODECK_NATIVE_TRAYTEST: close into the tray once, three seconds in, to check the way back
        if let Some(t) = trial {
            if t.elapsed() > Duration::from_secs(3) {
                trial = None;
                eprintln!("ectodeck-native: test: closing into the tray");
                app.hide();
            }
        }
        if let Some(n) = open_macros.filter(|_| app.front.page.dev.is_some()) {
            open_macros = None;
            app.open_macros(n);
        }
        if open_lab && app.board.is_some() {
            open_lab = false;
            app.open_lab();
        }
        app.act();
        let t = Instant::now();
        let before = app.frames;
        app.draw();
        app.draw_sub(MACROS);
        app.draw_sub(LAB);
        // ECTODECK_NATIVE_FPS: frames a second and the slowest, to check smoothness without slowing it
        if let Some((since, n, slow)) = &mut fps {
            if app.frames > before {
                *n += 1;
                *slow = (*slow).max(t.elapsed());
            }
            if since.elapsed() >= Duration::from_secs(1) {
                if *n > 0 {
                    eprintln!("ectodeck-native: {} frames a second, the slowest drawn in {:?}", n, slow);
                }
                (*since, *n, *slow) = (Instant::now(), 0, Duration::ZERO);
            }
        }
        // started again for a change to what Ectodeck may use, back on that place
        if let Some(place) = app.front.restart.take() {
            if let Ok(me) = std::env::current_exe() {
                let _ = std::process::Command::new(me).args(["--place", &place]).spawn();
            }
            app.exit = true;
        }
    }
    // a setting still being changed is written
    app.front.settings.flush(true);
    // a recording lets the keyboard go
    if let Some(m) = &mut app.front.macros {
        m.cancel_recording(&app.front.page);
    }
    // the keyboard's thread sends the preview's stop before the window goes
    std::thread::sleep(Duration::from_millis(300));
}

impl App {
    /// Do what the page asked.
    fn act(&mut self) {
        let asks = self.front.act();
        for ask in &asks {
            match ask {
                WindowAsk::Move if !asks.contains(&WindowAsk::Maximise) => {
                    if let Some(seat) = &self.seat {
                        self.window.move_(seat, self.press);
                    }
                    // the compositor keeps the button's release: tell the page it is up
                    let position = LogicalPosition::new(self.at.0 as f32, self.at.1 as f32);
                    self.slint.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
                    self.slint.dispatch_event(WindowEvent::PointerExited);
                }
                WindowAsk::Move => {}
                WindowAsk::Minimise => self.window.set_minimized(),
                WindowAsk::Maximise => {
                    if self.maximized {
                        self.window.unset_maximized()
                    } else {
                        self.window.set_maximized()
                    }
                }
                WindowAsk::Close => self.hide(),
                WindowAsk::OpenMacros(n) => self.open_macros(*n),
            }
        }
        let asks = [(MACROS, self.front.act_macros()), (LAB, self.front.act_lab())];
        for (which, asks) in asks {
            for ask in asks {
                let Some(sw) = &mut self.subs[which] else { break };
                match ask {
                    WindowAsk::Move => {
                        if let Some(seat) = &self.seat {
                            sw.window.move_(seat, self.press);
                        }
                        let position = LogicalPosition::new(sw.at.0 as f32, sw.at.1 as f32);
                        sw.slint.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
                        sw.slint.dispatch_event(WindowEvent::PointerExited);
                    }
                    WindowAsk::Minimise => sw.window.set_minimized(),
                    WindowAsk::Maximise => {
                        if sw.maximized {
                            sw.window.unset_maximized()
                        } else {
                            sw.window.set_maximized()
                        }
                    }
                    WindowAsk::Close => self.hide_sub(which),
                    WindowAsk::OpenMacros(_) => {}
                }
            }
        }
    }

    /// A window of its own beside the main one, for the Slint window made last.
    fn make_sub(&mut self, title: &str, size: (u32, u32), min: (u32, u32)) -> SubWin {
        let slint = self.made.borrow().last().cloned().expect("the window's adapter");
        slint.set_size(PhysicalSize::new(size.0, size.1));
        let surface = self.compositor.create_surface(&self.qh);
        let window = self._xdg.create_window(surface, WindowDecorations::RequestClient, &self.qh);
        window.set_title(title);
        window.set_app_id("com.ectonomic.ectodeck");
        window.set_min_size(Some(min));
        window.commit();
        SubWin { window, slint, size, scale: 1, configured: false, maximized: false, frame_pending: false, hidden: false, full: true, buffers: Vec::new(), px: Vec::new(), at: (0.0, 0.0) }
    }

    /// Show a window of its own again, as it was.
    fn show_sub(&mut self, which: usize) {
        if let Some(sw) = &mut self.subs[which] {
            if sw.hidden {
                sw.hidden = false;
                sw.full = true;
                sw.window.commit();
            }
        }
    }

    /// Open the Macro Editor's window, made the first time, on a macro.
    fn open_macros(&mut self, n: u32) {
        if self.front.macros.is_none() {
            let ui = MacroWindow::new().unwrap();
            let _ = slint::ComponentHandle::window(&ui);
            slint::ComponentHandle::show(&ui).unwrap();
            self.front.macros = Some(macro_page::MacroPage::new(ui, glyph(include_str!("../ui/glyphs/mark.svg"), 16, false)));
            self.subs[MACROS] = Some(self.make_sub("EVGA Z10 RGB / Macro Editor", MACRO_SIZE, MACRO_MIN_SIZE));
        } else {
            self.show_sub(MACROS);
        }
        if let Some(m) = &mut self.front.macros {
            m.open(&self.front.page, (n > 0).then_some(n));
        }
    }

    /// The light the keyboard is lit by: the Key lab's, if it has tuned
    /// one, or that of the setup in use (the core's, or one of the person's own).
    fn light_now(&self) -> serde_json::Value {
        self.light.clone().unwrap_or_else(|| cores::shown().device_now()["light"].clone())
    }

    /// Open the Key lab, made the first time, on the light the keyboard is lit by.
    fn open_lab(&mut self) {
        if self.front.lab.is_none() {
            let light = self.light_now();
            let ui = LabWindow::new().unwrap();
            let _ = slint::ComponentHandle::window(&ui);
            slint::ComponentHandle::show(&ui).unwrap();
            let setup = setups::in_use(&self.front.settings, &cores::shown().id).map(|s| s.id);
            self.front.lab = Some(lab::LabPage::new(ui, glyph(include_str!("../ui/glyphs/mark.svg"), 16, false), light, lab_device(self.board.as_ref()), setup));
            self.subs[LAB] = Some(self.make_sub(&format!("{} / Key lab", cores::shown().name()), LAB_SIZE, LAB_MIN_SIZE));
        } else {
            self.show_sub(LAB);
        }
        self.readout();
    }

    /// Tell the Key lab what is on the keyboard now, in whole nits, at
    /// most four times a second (an effect changes it every frame).
    fn readout(&mut self) {
        if self.readout_at.is_some_and(|t| t.elapsed() < Duration::from_millis(250)) {
            return;
        }
        let (Some(l), Some(b)) = (&mut self.front.lab, &self.board) else { return };
        self.readout_at = Some(Instant::now());
        let reference = self.pic.reference();
        let p = b.peaks(&self.lights, reference as f32, self.gain);
        let (photons, settled) = b.sim_progress().unwrap_or((0, false));
        l.set_readout(lab::Readout {
            legend: (p.legend as f64).round(),
            reach: (p.clean as f64).round(),
            deck: tenth(p.deck),
            skirt: tenth(p.skirt),
            chassis: tenth(p.chassis),
            lcd: (b.lcd_nits() as f64).min(self.gain as f64 * reference).round(),
            peak: (self.gain as f64 * reference).round(),
            white: reference.round(),
            photons,
            settled,
        });
    }

    /// Close a window of its own; it is kept, to open again as it was. The
    /// Macro Editor's lets a recording go.
    fn hide_sub(&mut self, which: usize) {
        if which == MACROS {
            if let Some(m) = &mut self.front.macros {
                m.cancel_recording(&self.front.page);
            }
        }
        let Some(sw) = &mut self.subs[which] else { return };
        sw.window.wl_surface().attach(None, 0, 0);
        sw.window.commit();
        sw.hidden = true;
        sw.configured = false;
        sw.frame_pending = false;
        sw.buffers.clear();
    }

    /// Draw a window of its own, when Slint has something new for it.
    fn draw_sub(&mut self, which: usize) {
        let Some(mw) = &mut self.subs[which] else { return };
        if mw.hidden || !mw.configured || mw.frame_pending {
            return;
        }
        let (w, h) = ((mw.size.0 * mw.scale) as usize, (mw.size.1 * mw.scale) as usize);
        if mw.px.len() != w * h {
            mw.px = vec![PremultipliedRgbaColor::default(); w * h];
            mw.full = true;
        }
        let full = mw.full;
        let mut dirty = None;
        let px = &mut mw.px;
        mw.slint.draw_if_needed(|r| {
            if full {
                r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            }
            let region = r.render(px, w);
            if full {
                r.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            }
            let (o, sz) = (region.bounding_box_origin(), region.bounding_box_size());
            if sz.width > 0 && sz.height > 0 {
                dirty = Some((o.x.max(0), o.y.max(0), sz.width as i32, sz.height as i32));
            }
        });
        let dirty = if full { Some((0, 0, w as i32, h as i32)) } else { dirty };
        let Some((dx, dy, dw, dh)) = dirty else { return };
        mw.full = false;
        let changed = [(dx.max(0) as usize, dy.max(0) as usize, dw.max(0) as usize, dh.max(0) as usize)];
        let (i, todo) = free_buffer(&mut mw.buffers, &mut self.pool, (w, h), w * 4, wl_shm::Format::Argb8888, &changed);
        let canvas = mw.buffers[i].0.canvas(&mut self.pool).unwrap();
        // the compositor's ARGB, which little-endian memory holds as B, G, R, A;
        // only what is out of date in this buffer
        for &(x, y, rw, rh) in &todo {
            let (x1, y1) = ((x + rw).min(w), (y + rh).min(h));
            for row in y.min(y1)..y1 {
                let (a, b) = (row * w + x.min(x1), row * w + x1);
                for (d, p) in canvas[a * 4..b * 4].chunks_exact_mut(4).zip(&mw.px[a..b]) {
                    d.copy_from_slice(&[p.blue, p.green, p.red, p.alpha]);
                }
            }
        }
        let surface = mw.window.wl_surface();
        surface.damage_buffer(dx, dy, dw, dh);
        surface.set_buffer_scale(mw.scale as i32);
        surface.frame(&self.qh, surface.clone());
        mw.buffers[i].0.attach_to(surface).unwrap();
        mw.window.commit();
        mw.frame_pending = true;
        if let Some(dir) = &self.dump {
            let rgba: Vec<u8> = mw.px.iter().flat_map(|p| [p.red, p.green, p.blue, p.alpha]).collect();
            let name = if which == MACROS { "macros" } else { "lab" };
            let _ = std::fs::write(dir.join(format!("{name}.raw")), rgba);
            let _ = std::fs::write(dir.join(format!("{name}.txt")), format!("{w} {h}\n"));
        }
    }

    /// Which window of its own a surface is, if any.
    fn which_sub(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        (0..self.subs.len()).find(|&i| self.subs[i].as_ref().is_some_and(|m| m.window.wl_surface() == surface))
    }

    /// The pointer over a window of its own.
    fn sub_pointer(&mut self, which: usize, e: &PointerEvent) {
        let Some(mw) = &mut self.subs[which] else { return };
        let (x, y) = e.position;
        mw.at = (x, y);
        let position = LogicalPosition::new(x as f32, y as f32);
        match e.kind {
            PointerEventKind::Enter { .. } => {
                self.cursor = None;
                mw.slint.dispatch_event(WindowEvent::PointerMoved { position });
            }
            PointerEventKind::Leave { .. } => mw.slint.dispatch_event(WindowEvent::PointerExited),
            PointerEventKind::Motion { .. } => mw.slint.dispatch_event(WindowEvent::PointerMoved { position }),
            PointerEventKind::Press { button: b, serial, .. } => {
                self.press = serial;
                if b == 0x110 {
                    if let (Some(edge), Some(seat)) = (edge(mw.size, mw.maximized, x, y), &self.seat) {
                        mw.window.resize(seat, serial, edge);
                        return;
                    }
                }
                if which == MACROS {
                    if let Some(m) = &mut self.front.macros {
                        m.pressed_at(x as f32, y as f32);
                    }
                }
                mw.slint.dispatch_event(WindowEvent::PointerPressed { position, button: pointer_button(b) });
            }
            PointerEventKind::Release { button: b, .. } => mw.slint.dispatch_event(WindowEvent::PointerReleased { position, button: pointer_button(b) }),
            PointerEventKind::Axis { horizontal, vertical, .. } => mw.slint.dispatch_event(WindowEvent::PointerScrolled {
                position,
                delta_x: -horizontal.absolute as f32,
                delta_y: -vertical.absolute as f32,
            }),
        }
        let icon = edge_cursor(edge(mw.size, mw.maximized, x, y)).unwrap_or(cursor_icon(mw.slint.pointer()));
        self.set_cursor(icon);
    }

    /// The screen the window is on has new levels: describe the picture for
    /// them, for the white its user set where KDE says it (an idle dim does
    /// not change it; see kde_output::white). The new description takes over
    /// once KWin has it ready.
    fn retarget(&mut self, qh: &QueueHandle<Self>) {
        let Some((id, reported, peak, undimmed)) = self.screens.iter().find(|s| Some(s.id) == self.on_screen).map(|s| (s.id, s.reference, s.peak, s.undimmed)) else { return };
        if reported <= 0.0 {
            return;
        }
        let white = kde_output::white(reported, self.kde_of(id), undimmed);
        self.levels_peak = peak.max(white);
        let glow = self.glow_limit.unwrap_or(peak).max(white);
        if (white - self.levels.0).abs() < 0.5 && (glow - self.levels.1).abs() < 0.5 {
            return;
        }
        eprintln!("ectodeck-native: the screen's white is now {white} nits; the light goes to {glow}");
        describe(&self.cm, qh, white, self.levels_peak, self.mastering, PictureTag(white, glow));
    }

    /// What KDE says of a screen, by its output's id: the one of its name.
    fn kde_of(&self, id: u32) -> Option<&kde_output::Device> {
        let name = self.output_name(id)?;
        self.kde.iter().map(|k| &k.1).find(|d| d.name == name)
    }

    /// A screen's name, by its output's id.
    fn output_name(&self, id: u32) -> Option<String> {
        use smithay_client_toolkit::reexports::client::Proxy;
        let output = self.outputs.outputs().find(|o| o.id().protocol_id() == id)?;
        self.outputs.info(&output)?.name
    }

    /// Close the window into the tray: without its buffer the window is
    /// unmapped, and Ectodeck carries on. With no tray, closing quits.
    fn hide(&mut self) {
        self.front.stop_preview();
        if self.tray.is_none() {
            self.exit = true;
            return;
        }
        self.window.wl_surface().attach(None, 0, 0);
        self.window.commit();
        self.hidden = true;
        self.configured = false;
        self.frame_pending = false;
        self.buffers.clear();
    }

    /// Bring the window back: a first commit with no buffer, as when it was
    /// made, and the compositor answers with its size.
    fn show(&mut self) {
        if !self.hidden {
            return;
        }
        eprintln!("ectodeck-native: shown again from the tray");
        self.hidden = false;
        self.full = true;
        self.window.commit();
    }

    /// The keyboard came back drawn at a size: light it, by the Key lab's
    /// light if it has tuned one.
    fn built(&mut self, mut board: Board) {
        // the Key lab, open before the device was first drawn, takes its caps
        if let Some(l) = self.front.lab.as_mut().filter(|l| !l.has_caps()) {
            l.set_caps(lab_device(Some(&board)).caps);
        }
        if let Some(l) = &self.light {
            board.set_light(l);
        }
        let now = self.light_now();
        self.front.light_caps(&now);
        // (drawn from the core as it was read: the legends the light asks for now)
        if let Some(ask) = board.legends_for(&self.light_now()) {
            let _ = self.set_legends.send(ask);
        }
        // The keys' light, simulated from the first keyboard drawn on, for
        // every size (the simulation is in the art's units). Its first
        // picture, the light straight from the LEDs, is waited for, a tenth
        // of a second or so, so the keys don't show dark first.
        if self.sim.is_none() {
            let tx = std::sync::Mutex::new(self.sim_tx.clone());
            let s = Simulator::new(Scene::of(&board), board.setup(&self.light_now()), Box::new(move || {
                let _ = tx.lock().map(|t| t.send(()));
            }));
            let t = Instant::now();
            s.wait(Duration::from_millis(1500));
            eprintln!("ectodeck-native: the keys' light simulated, its first picture in {:?}", t.elapsed());
            self.sim = Some(s);
        }
        let want = board.setup(&self.light_now());
        if let Some(snap) = self.kept.get(&want) {
            board.set_sim(snap);
        } else if let Some(s) = &self.sim {
            // (the simulation's latest, though it may be of the make before:
            // the keyboard drawn new has none to keep instead)
            s.set(&want);
            if let Some(snap) = s.latest() {
                self.sim_seen = s.version();
                board.set_sim(snap);
            }
        }
        self.want = Some(want);
        let lit = light(&board, &self.lights, self.front.page.locks, self.gain, self.pic.reference(), self.front.overlays.panel_for(board.w, board.lcd().dot));
        self.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
        self.board = Some(board);
        self.full = true;
        self.draw();
    }

    /// Legends set as the light asks: put them on the keyboard, and light it again.
    fn legends_ready(&mut self, legends: ectodeck_draw::hdr::Legends) {
        let Some(board) = &mut self.board else { return };
        board.set_legends(legends);
        // (the light may have changed again while they were set)
        if let Some(ask) = board.legends_for(&self.light.clone().unwrap_or_else(|| cores::shown().device_now()["light"].clone())) {
            let _ = self.set_legends.send(ask);
        }
        let lit = light(board, &self.lights, self.front.page.locks, self.gain, self.pic.reference(), self.front.overlays.panel_for(board.w, board.lcd().dot));
        self.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
        self.full = true;
        self.draw();
    }

    /// The simulation has a clearer picture of the keys' light: light the keyboard by it.
    fn sim_ready(&mut self) {
        let Some(s) = &self.sim else { return };
        let version = s.version();
        let (Some(snap), Some(board)) = (s.latest(), &mut self.board) else { return };
        if version == self.sim_seen {
            return;
        }
        self.sim_seen = version;
        if !Kept::takes(self.want.as_ref(), board.sim(), &snap) {
            return;
        }
        if snap.settled {
            self.kept.keep(snap.clone());
        }
        board.set_sim(snap);
        let lit = light(board, &self.lights, self.front.page.locks, self.gain, self.pic.reference(), self.front.overlays.panel_for(board.w, board.lcd().dot));
        self.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
        self.readout_at = None;
        self.readout();
        self.draw();
    }

    /// How long between a lighting effect's frames: Settings › Performance's
    /// 30 or 60 a second, or none at all to match the screen.
    fn effect_frame(&self) -> Duration {
        match self.front.settings.get("performance.effects").and_then(|v| v.as_str().map(str::to_owned)).as_deref() {
            Some("screen") => Duration::ZERO,
            Some("60") => Duration::from_secs_f64(1.0 / 60.0),
            _ => Duration::from_secs_f64(1.0 / 30.0),
        }
    }

    fn draw(&mut self) {
        // the keys' light as bright as Settings › HDR lets it go
        self.front.levels = (self.pic.reference(), self.levels_peak);
        let g = self.front.light_gain(self.pic.reference());
        if (g - self.gain).abs() > 1e-3 {
            self.gain = g;
            if let Some(board) = &self.board {
                let lit = light(board, &self.lights, self.front.page.locks, self.gain, self.pic.reference(), self.front.overlays.panel_for(board.w, board.lcd().dot));
                self.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
            }
        }
        if let Some(t) = &self.tray {
            let on = self.front.ui.get_conn_state() == 1;
            if on != self.tray_connected {
                self.tray_connected = on;
                t.connected(on);
            }
        }
        if self.hidden || !self.configured || self.frame_pending {
            return;
        }
        let (w, h) = ((self.size.0 * self.scale) as usize, (self.size.1 * self.scale) as usize);
        if self.pic.w != w || self.pic.h != h {
            self.pic.resize(w, h);
            self.full = true;
        }
        // the knobs and the display's glass for the keyboard's size on the page,
        // before the page is drawn, so they move with it in the same frame
        let rect = board_rect(&self.front.ui, self.scale as f32);
        self.front.board_size(rect.2 as u32, rect.3 as u32, self.scale as f32);
        // the Key lab's tuning, when it has changed
        let mut relit = false;
        let mut relit_caps = false;
        if let Some(lab) = &self.front.lab {
            let rev = (lab.rev, lab.comparing);
            if rev != self.light_rev {
                self.light_rev = rev;
                self.light = Some(lab.shown().clone());
                if let (Some(b), Some(l)) = (&mut self.board, &self.light) {
                    // the keys drawn again only if their picture changes now; how
                    // the light falls shows with the simulation's next picture,
                    // or at once from a picture kept of it
                    relit = b.set_light(l);
                    // the legends, as the light asks for them, when they are set
                    if let Some(ask) = b.legends_for(l) {
                        let _ = self.set_legends.send(ask);
                    }
                    let want = b.setup(l);
                    self.kept.pinned = Some(b.setup(lab.saved()));
                    let changed = self.want.as_ref().is_none_or(|w| !w.same_paths(&want));
                    if changed {
                        if let Some(snap) = self.kept.get(&want) {
                            b.set_sim(snap);
                            relit = true;
                            if let Some(s) = &self.sim {
                                s.rest();
                            }
                        } else if let Some(s) = &self.sim {
                            s.set(&want);
                        }
                    }
                    self.want = Some(want);
                }
                self.readout_at = None;
                // (the caps' colours, for what is printed over the keys)
                relit_caps = true;
            }
        }
        if relit_caps {
            let now = self.light_now();
            self.front.light_caps(&now);
            self.front.refresh();
        }
        // the keys' light, or the display's text, when it has changed
        if let Some(b) = &self.board {
            let _ = self.front.overlays.panel_for(b.w, b.lcd().dot);
            // the glass's sheen as the core sets it
            let glass = b.lcd().glass;
            if (glass - self.front.overlays.glass_opacity).abs() > 1e-4 {
                self.front.overlays.glass_opacity = glass;
                self.front.show_overlays();
            }
        }
        let panel_moved = self.front.overlays.panel_rev != self.panel_rev || relit;
        self.panel_rev = self.front.overlays.panel_rev;
        // a lighting effect's next frame only when it is due, at the rate
        // Settings › Performance asks for; anything else as it changes
        let effect = self.front.animated();
        let due = !effect || Instant::now() >= self.effect_due;
        let lights = if due { self.front.lights() } else { None };
        if effect && lights.is_some() {
            self.effect_due = Instant::now() + self.effect_frame();
        }
        // the last frame's picture, kept until the picture shows the new one
        let mut shown_before = None;
        if lights.is_some() || panel_moved {
            if let Some(l) = lights {
                self.lights = l;
            }
            if let Some(board) = &self.board {
                let mut px = std::mem::take(&mut self.spare);
                light_into(&mut px, board, &self.lights, self.front.page.locks, self.gain, self.pic.reference(), self.front.overlays.panel_for(board.w, board.lcd().dot));
                shown_before = self.lit.replace(Rc::new(Lit { px, w: board.w as usize, h: board.h as usize }));
            }
            self.readout();
        }
        let full = self.full;
        let mut dirty = Vec::new();
        let pic = &mut self.pic;
        self.slint.draw_if_needed(|r| {
            if full {
                r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            }
            let region = r.render(&mut pic.ui, w);
            if full {
                r.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            }
            // the whole of what Slint changed: its list of rectangles has been
            // seen to leave out pixels it drew (the title bar's buttons)
            let (o, sz) = (region.bounding_box_origin(), region.bounding_box_size());
            if sz.width > 0 && sz.height > 0 {
                dirty.push((o.x.max(0) as usize, o.y.max(0) as usize, sz.width as usize, sz.height as usize));
            }
        });

        if lay_ground(&mut self.pic, &self.front.ui, self.scale as f32) {
            self.full = true;
        }
        // the keyboard: the one drawn at this size, or the last one stretched to
        // it while this size is drawn
        let rect = board_rect(&self.front.ui, self.scale as f32);
        // drawn for the window's size even while another page shows, so the
        // keyboard is there, whole, the moment its page opens
        let ui = &self.front.ui;
        let s = self.scale as f32;
        let want = ((ui.get_board_width() * s).round().max(1.0) as u32, (ui.get_board_height() * s).round().max(1.0) as u32);
        if want.0 > 1 && self.board.as_ref().map(|b| (b.w, b.h)) != Some(want) && self.asked != Some(want) {
            self.asked = Some(want);
            let _ = self.build.send(want);
        }
        self.front.board_size(rect.2 as u32, rect.3 as u32, self.scale as f32);
        self.front.ui.set_board_ready(self.lit.is_some());
        let before = self.pic.keyboard_rect();
        if self.pic.set_keyboard(rect, self.lit.as_ref()) {
            for r in [before, rect] {
                dirty.push((r.0.max(0) as usize, r.1.max(0) as usize, r.2, r.3));
            }
        }
        // the last frame's picture taken back, now nothing shows it, for the next
        if let Some(old) = shown_before {
            if let Ok(l) = Rc::try_unwrap(old) {
                self.spare = l.px;
            }
        }

        if self.full {
            self.pic.encode_all();
            dirty = vec![(0, 0, w, h)];
        } else if dirty.is_empty() {
            return;
        } else {
            for &(x, y, dw, dh) in &dirty {
                self.pic.encode(x, y, dw, dh);
            }
            if self.check {
                let parts = self.pic.out.clone();
                self.pic.encode_all();
                let w = self.pic.w;
                let (mut n, mut bb) = (0, (usize::MAX, usize::MAX, 0, 0));
                for (i, (a, b)) in parts.iter().zip(&self.pic.out).enumerate() {
                    if a != b {
                        n += 1;
                        let (x, y) = (i % w, i / w);
                        bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x), bb.3.max(y));
                    }
                }
                if n > 0 {
                    eprintln!("ectodeck-native: CHECK frame {}: {n} pixels differ from a whole encode, in x {}..{} y {}..{}; encoded {:?}; keyboard at {:?}", self.frames, bb.0, bb.2, bb.1, bb.3, dirty, self.pic.keyboard_rect());
                }
            }
        }
        self.full = false;
        self.present(&dirty);
    }

    /// Hand the picture to the compositor.
    fn present(&mut self, dirty: &[(usize, usize, usize, usize)]) {
        let (w, h) = (self.pic.w, self.pic.h);
        let (i, todo) = free_buffer(&mut self.buffers, &mut self.pool, (w, h), w * 8, wl_shm::Format::Abgr16161616, dirty);
        let canvas = self.buffers[i].0.canvas(&mut self.pool).unwrap();
        let src: &[u8] = bytemuck::cast_slice(&self.pic.out);
        for &(x, y, rw, rh) in &todo {
            let (x1, y1) = ((x + rw).min(w), (y + rh).min(h));
            for row in y.min(y1)..y1 {
                let (a, b) = ((row * w + x.min(x1)) * 8, (row * w + x1) * 8);
                canvas[a..b].copy_from_slice(&src[a..b]);
            }
        }
        let surface = self.window.wl_surface();
        for &(x, y, dw, dh) in dirty {
            surface.damage_buffer(x as i32, y as i32, dw as i32, dh as i32);
        }
        surface.set_buffer_scale(self.scale as i32);
        surface.frame(&self.qh, surface.clone());
        self.buffers[i].0.attach_to(surface).unwrap();
        self.window.commit();
        self.frame_pending = true;
        self.frames += 1;
        if let Some(dir) = &self.dump {
            let _ = std::fs::write(dir.join("frame.raw"), bytemuck::cast_slice::<[u16; 4], u8>(&self.pic.out));
            let _ = std::fs::write(dir.join("frame.txt"), format!("{} {} {} {}\n", w, h, self.frames, self.pic.reference()));
        }
    }

    fn set_cursor(&mut self, icon: CursorIcon) {
        if self.cursor == Some(icon) {
            return;
        }
        if let Some(p) = &self.pointer {
            if p.set_cursor(&self.conn, icon).is_ok() {
                self.cursor = Some(icon);
            }
        }
    }

    /// The window's edge under the pointer, which a press resizes from.
    fn edge(&self, x: f64, y: f64) -> Option<ResizeEdge> {
        edge(self.size, self.maximized, x, y)
    }

    /// A key on the keyboard, pressed or let go.
    fn key(&mut self, e: &KeyEvent, down: bool) {
        // Ctrl+Shift+L, anywhere in Ectodeck: the Key lab, a developer's panel
        if down && self.mods.ctrl && self.mods.shift && matches!(e.keysym, Keysym::l | Keysym::L) {
            self.open_lab();
            return;
        }
        if self.keys_on == Some(LAB) && down && self.mods.ctrl && matches!(e.keysym, Keysym::z | Keysym::Z | Keysym::y | Keysym::Y) {
            if let Some(lab) = self.front.lab.as_mut().filter(|l| !l.typing()) {
                lab.undo(self.mods.shift || matches!(e.keysym, Keysym::y | Keysym::Y));
                return;
            }
        }
        if let Some(which) = self.keys_on {
            if let (Some(sw), Some(text)) = (&self.subs[which], key_text(e)) {
                sw.slint.dispatch_event(if down { WindowEvent::KeyPressed { text } } else { WindowEvent::KeyReleased { text } });
            }
            return;
        }
        if self.front.typed(e.raw_code, down) {
            self.front.refresh();
            return;
        }
        if let Some(text) = key_text(e) {
            self.slint.dispatch_event(if down { WindowEvent::KeyPressed { text } } else { WindowEvent::KeyReleased { text } });
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, factor: i32) {
        if let Some(which) = self.which_sub(surface) {
            let mw = self.subs[which].as_mut().unwrap();
            mw.scale = factor.max(1) as u32;
            mw.slint.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: mw.scale as f32 });
            mw.slint.set_size(PhysicalSize::new(mw.size.0 * mw.scale, mw.size.1 * mw.scale));
            mw.full = true;
            // the Key lab's legends drawn again at the screen's own pixels
            if which == LAB {
                if let Some(l) = &mut self.front.lab {
                    l.refresh();
                }
            }
            return;
        }
        self.scale = factor.max(1) as u32;
        self.slint.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: self.scale as f32 });
        self.slint.set_size(PhysicalSize::new(self.size.0 * self.scale, self.size.1 * self.scale));
        self.full = true;
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        if let Some(which) = self.which_sub(surface) {
            self.subs[which].as_mut().unwrap().frame_pending = false;
        } else {
            self.frame_pending = false;
        }
    }
    fn surface_enter(&mut self, _: &Connection, qh: &QueueHandle<Self>, surface: &wl_surface::WlSurface, output: &wl_output::WlOutput) {
        if surface != self.window.wl_surface() {
            return;
        }
        use smithay_client_toolkit::reexports::client::Proxy;
        self.on_screen = Some(output.id().protocol_id());
        self.retarget(qh);
    }
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        use smithay_client_toolkit::reexports::client::Proxy;
        // ask KWin for the screen's levels, now and whenever they change
        let id = output.id().protocol_id();
        let cmo = self.cm.get_output(&output, qh, ScreenTag(id));
        cmo.get_image_description(qh, ScreenTag(id));
        self.screens.push(ScreenLevels { id, cmo, reference: 0.0, peak: 0.0, reading: (0.0, 0.0), undimmed: None });
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(which) = self.which_sub(window.wl_surface()) {
            self.hide_sub(which);
        } else {
            self.hide();
        }
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window, configure: WindowConfigure, _: u32) {
        if let Some(which) = self.which_sub(window.wl_surface()) {
            let mw = self.subs[which].as_mut().unwrap();
            let w = configure.new_size.0.map_or(mw.size.0, NonZeroU32::get);
            let h = configure.new_size.1.map_or(mw.size.1, NonZeroU32::get);
            if (w, h) != mw.size || !mw.configured {
                mw.size = (w, h);
                mw.slint.set_size(PhysicalSize::new(w * mw.scale, h * mw.scale));
                mw.full = true;
            }
            mw.maximized = configure.is_maximized();
            let maximized = mw.maximized;
            window.xdg_surface().set_window_geometry(0, 0, w as i32, h as i32);
            mw.configured = true;
            // the page laid out again for the window's width: the macro's chain wraps at it
            if which == MACROS {
                if let Some(m) = &mut self.front.macros {
                    m.ui.set_is_maximized(maximized);
                    m.refresh(&self.front.page);
                }
            } else if let Some(l) = &mut self.front.lab {
                l.ui.set_is_maximized(maximized);
                l.refresh();
            }
            self.draw_sub(which);
            return;
        }
        let w = configure.new_size.0.map_or(self.size.0, NonZeroU32::get);
        let h = configure.new_size.1.map_or(self.size.1, NonZeroU32::get);
        if (w, h) != self.size || !self.configured {
            self.size = (w, h);
            self.slint.set_size(PhysicalSize::new(w * self.scale, h * self.scale));
            self.full = true;
        }
        let maximized = configure.is_maximized();
        if maximized != self.maximized {
            self.maximized = maximized;
            self.front.ui.set_is_maximized(maximized);
        }
        window.xdg_surface().set_window_geometry(0, 0, w as i32, h as i32);
        self.configured = true;
        self.draw();
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor.create_surface(qh);
            self.pointer = self.seats.get_pointer_with_theme(qh, &seat, self.shm.wl_shm(), surface, ThemeSpec::default()).ok();
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
        self.seat = Some(seat);
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.pointer().release();
            }
        }
        if capability == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {
        self.keys_on = self.which_sub(surface);
        match self.keys_on.and_then(|w| self.subs[w].as_ref()) {
            Some(sw) => sw.slint.dispatch_event(WindowEvent::WindowActiveChanged(true)),
            None => self.slint.dispatch_event(WindowEvent::WindowActiveChanged(true)),
        }
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface, _: u32) {
        match self.which_sub(surface).and_then(|w| self.subs[w].as_ref()) {
            Some(sw) => sw.slint.dispatch_event(WindowEvent::WindowActiveChanged(false)),
            None => self.slint.dispatch_event(WindowEvent::WindowActiveChanged(false)),
        }
        self.keys_on = None;
    }
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(&event, true);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(&event, false);
    }
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, mods: Modifiers, _: u32) {
        self.mods = mods;
    }
}

impl PointerHandler for App {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        const BTN_LEFT: u32 = 0x110;
        const BTN_RIGHT: u32 = 0x111;
        const BTN_MIDDLE: u32 = 0x112;
        for e in events {
            if let Some(which) = self.which_sub(&e.surface) {
                self.sub_pointer(which, e);
                continue;
            }
            if &e.surface != self.window.wl_surface() {
                continue;
            }
            let (x, y) = e.position;
            self.at = (x, y);
            let position = LogicalPosition::new(x as f32, y as f32);
            let button = |b: u32| match b {
                BTN_LEFT => PointerEventButton::Left,
                BTN_RIGHT => PointerEventButton::Right,
                BTN_MIDDLE => PointerEventButton::Middle,
                _ => PointerEventButton::Other,
            };
            match e.kind {
                PointerEventKind::Enter { .. } => {
                    self.cursor = None;
                    self.slint.dispatch_event(WindowEvent::PointerMoved { position });
                }
                PointerEventKind::Leave { .. } => {
                    self.slint.dispatch_event(WindowEvent::PointerExited);
                }
                PointerEventKind::Motion { .. } => {
                    self.slint.dispatch_event(WindowEvent::PointerMoved { position });
                }
                PointerEventKind::Press { button: b, serial, .. } => {
                    self.press = serial;
                    if b == BTN_LEFT {
                        if let (Some(edge), Some(seat)) = (self.edge(x, y), &self.seat) {
                            self.window.resize(seat, serial, edge);
                            continue;
                        }
                    }
                    self.front.pressed_at(x as f32, y as f32);
                    self.slint.dispatch_event(WindowEvent::PointerPressed { position, button: button(b) });
                }
                PointerEventKind::Release { button: b, .. } => {
                    self.slint.dispatch_event(WindowEvent::PointerReleased { position, button: button(b) });
                }
                PointerEventKind::Axis { horizontal, vertical, .. } => {
                    self.slint.dispatch_event(WindowEvent::PointerScrolled {
                        position,
                        delta_x: -horizontal.absolute as f32,
                        delta_y: -vertical.absolute as f32,
                    });
                }
            }
            // the cursor: resizing at the edges, the hand over what can be clicked
            let icon = match self.edge(x, y) {
                Some(ResizeEdge::TopLeft) => CursorIcon::NwResize,
                Some(ResizeEdge::TopRight) => CursorIcon::NeResize,
                Some(ResizeEdge::BottomLeft) => CursorIcon::SwResize,
                Some(ResizeEdge::BottomRight) => CursorIcon::SeResize,
                Some(ResizeEdge::Left) => CursorIcon::WResize,
                Some(ResizeEdge::Right) => CursorIcon::EResize,
                Some(ResizeEdge::Top) => CursorIcon::NResize,
                Some(ResizeEdge::Bottom) => CursorIcon::SResize,
                _ => cursor_icon(self.slint.pointer()),
            };
            self.set_cursor(icon);
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<WpImageDescriptionV1, ()> for App {
    fn event(app: &mut Self, _: &WpImageDescriptionV1, e: wp_image_description_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match e {
            wp_image_description_v1::Event::Ready { .. } | wp_image_description_v1::Event::Ready2 { .. } => app.ready = true,
            wp_image_description_v1::Event::Failed { msg, .. } => app.failed = Some(msg),
            _ => {}
        }
    }
}

impl Dispatch<WpImageDescriptionV1, PictureTag> for App {
    fn event(app: &mut Self, desc: &WpImageDescriptionV1, e: wp_image_description_v1::Event, tag: &PictureTag, _: &Connection, _: &QueueHandle<Self>) {
        if let wp_image_description_v1::Event::Ready { .. } | wp_image_description_v1::Event::Ready2 { .. } = e {
            // the description and the pixels drawn for it go to KWin together, on the next frame
            let PictureTag(reference, glow) = *tag;
            app.cm_surface.set_image_description(desc, wp_color_manager_v1::RenderIntent::Perceptual);
            app.levels = (reference, glow);
            app.pic.set_reference(reference);
            app.gain = LIGHT_GAIN;
            if let Some(board) = &app.board {
                let lit = light(board, &app.lights, app.front.page.locks, app.gain, app.pic.reference(), app.front.overlays.panel_for(board.w, board.lcd().dot));
                app.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
            }
            app.full = true;
            app.slint.request_redraw();
        }
    }
}

/// A screen's description, asked for its levels once KWin has it ready: a
/// screen turned off has none, and asking a failed description for its
/// levels is a protocol error, which ends the window's connection.
impl Dispatch<WpImageDescriptionV1, ScreenTag> for App {
    fn event(_: &mut Self, desc: &WpImageDescriptionV1, e: wp_image_description_v1::Event, tag: &ScreenTag, _: &Connection, qh: &QueueHandle<Self>) {
        match e {
            wp_image_description_v1::Event::Ready { .. } | wp_image_description_v1::Event::Ready2 { .. } => {
                desc.get_information(qh, *tag);
                desc.destroy();
            }
            wp_image_description_v1::Event::Failed { .. } => desc.destroy(),
            _ => {}
        }
    }
}

impl Dispatch<WpColorManagementOutputV1, ScreenTag> for App {
    fn event(_: &mut Self, cmo: &WpColorManagementOutputV1, e: wp_color_management_output_v1::Event, tag: &ScreenTag, _: &Connection, qh: &QueueHandle<Self>) {
        if let wp_color_management_output_v1::Event::ImageDescriptionChanged = e {
            cmo.get_image_description(qh, *tag);
        }
    }
}

impl Dispatch<WpImageDescriptionInfoV1, ScreenTag> for App {
    fn event(app: &mut Self, _: &WpImageDescriptionInfoV1, e: wp_image_description_info_v1::Event, tag: &ScreenTag, _: &Connection, qh: &QueueHandle<Self>) {
        let Some(s) = app.screens.iter_mut().find(|s| s.id == tag.0) else { return };
        match e {
            wp_image_description_info_v1::Event::Luminances { max_lum, reference_lum, .. } => s.reading = (reference_lum as f64, max_lum as f64),
            wp_image_description_info_v1::Event::Done => {
                (s.reference, s.peak) = s.reading;
                // its white undimmed, for while it is dimmed
                let id = tag.0;
                let dimmed = app.kde_of(id).is_some_and(|d| d.dimmed());
                if let Some(s) = app.screens.iter_mut().find(|s| s.id == id && !dimmed && s.reference > 0.0) {
                    s.undimmed = Some(s.reference);
                }
                if app.on_screen == Some(id) {
                    app.retarget(qh);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<KdeOutputDeviceRegistryV2, ()> for App {
    fn event(app: &mut Self, _: &KdeOutputDeviceRegistryV2, e: kde_output_device_registry_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let kde_output_device_registry_v2::Event::Output { output } = e {
            app.kde.push((output, kde_output::Device::default(), kde_output::Device::default()));
        }
    }
    smithay_client_toolkit::reexports::client::event_created_child!(App, KdeOutputDeviceRegistryV2, [
        kde_output_device_registry_v2::EVT_OUTPUT_OPCODE => (KdeOutputDeviceV2, ()),
    ]);
}

impl Dispatch<KdeOutputDeviceV2, ()> for App {
    fn event(app: &mut Self, dev: &KdeOutputDeviceV2, e: kde_output_device_v2::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        let Some(k) = app.kde.iter_mut().find(|k| &k.0 == dev) else { return };
        // A screen's word is whole at its done. KWin does not say when an
        // idle dim changes its white, but KDE says the dim: ask KWin for the
        // screen's levels again (their information retargets the window's).
        if k.2.take(e) {
            k.1 = k.2.clone();
            let name = k.1.name.clone();
            let ids: Vec<u32> = app.screens.iter().map(|s| s.id).filter(|&id| app.output_name(id).as_deref() == Some(name.as_str())).collect();
            for s in app.screens.iter().filter(|s| ids.contains(&s.id)) {
                s.cmo.get_image_description(qh, ScreenTag(s.id));
            }
            app.retarget(qh);
        }
    }
    smithay_client_toolkit::reexports::client::event_created_child!(App, KdeOutputDeviceV2, [
        kde_output_device_v2::EVT_MODE_OPCODE => (KdeOutputDeviceModeV2, ()),
    ]);
}

delegate_noop!(App: ignore KdeOutputDeviceModeV2);

impl Dispatch<WpColorManagerV1, ()> for App {
    fn event(app: &mut Self, _: &WpColorManagerV1, e: wp_color_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use smithay_client_toolkit::reexports::client::WEnum;
        if let wp_color_manager_v1::Event::SupportedFeature { feature: WEnum::Value(wp_color_manager_v1::Feature::SetMasteringDisplayPrimaries) } = e {
            app.mastering = true;
        }
    }
}
delegate_noop!(App: ignore WpColorManagementSurfaceV1);
delegate_noop!(App: ignore WpImageDescriptionCreatorParamsV1);
delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_pointer!(App);
delegate_keyboard!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_registry!(App);

/// A window's edge under the pointer, which a press resizes from.
fn edge(size: (u32, u32), maximized: bool, x: f64, y: f64) -> Option<ResizeEdge> {
    if maximized {
        return None;
    }
    let (w, h) = (size.0 as f64, size.1 as f64);
    let (l, r, t, b) = (x < EDGE, x > w - EDGE, y < EDGE, y > h - EDGE);
    Some(match (l, r, t, b) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (_, true, true, _) => ResizeEdge::TopRight,
        (true, _, _, true) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, ..) => ResizeEdge::Left,
        (_, true, ..) => ResizeEdge::Right,
        (_, _, true, _) => ResizeEdge::Top,
        (_, _, _, true) => ResizeEdge::Bottom,
        _ => return None,
    })
}

fn pointer_button(b: u32) -> PointerEventButton {
    match b {
        0x110 => PointerEventButton::Left,
        0x111 => PointerEventButton::Right,
        0x112 => PointerEventButton::Middle,
        _ => PointerEventButton::Other,
    }
}

/// The cursor for resizing from an edge.
fn edge_cursor(e: Option<ResizeEdge>) -> Option<CursorIcon> {
    Some(match e? {
        ResizeEdge::TopLeft => CursorIcon::NwResize,
        ResizeEdge::TopRight => CursorIcon::NeResize,
        ResizeEdge::BottomLeft => CursorIcon::SwResize,
        ResizeEdge::BottomRight => CursorIcon::SeResize,
        ResizeEdge::Left => CursorIcon::WResize,
        ResizeEdge::Right => CursorIcon::EResize,
        ResizeEdge::Top => CursorIcon::NResize,
        ResizeEdge::Bottom => CursorIcon::SResize,
        _ => return None,
    })
}

/// The compositor's cursor for the one Slint asked for.
fn cursor_icon(p: Pointer) -> CursorIcon {
    match p {
        Pointer::Hand => CursorIcon::Pointer,
        Pointer::Text => CursorIcon::Text,
        Pointer::Grab => CursorIcon::Grab,
        Pointer::Grabbing => CursorIcon::Grabbing,
        Pointer::Default => CursorIcon::Default,
    }
}

/// The device the Key lab tunes, as the shown core describes it: its
/// name, where its description is, which of the light's parts it has, and
/// (once its board is drawn) every cap, each in section as the art has it.
fn lab_device(board: Option<&Board>) -> lab::LabDevice {
    let core = cores::shown();
    let light = core.device_now()["light"].clone();
    let mm = sim::Physics::from(&light).units_per_mm;
    let typical = light["typical"].as_str().map(str::to_string).or_else(|| light["typical"].as_u64().map(|n| n.to_string()));
    lab::LabDevice {
        name: core.name().to_string(),
        core: core.id.clone(),
        path: core.path.clone(),
        // (before its board is drawn, a device with art is taken to have keys and wells)
        keys: board.is_none_or(|b| b.cap_shapes.iter().any(|c| c.lit)),
        wells: board.is_none_or(|b| b.has_wells()),
        screens: core.device["screens"].as_array().is_some_and(|a| !a.is_empty()),
        caps: board.map(|b| b.cap_shapes.iter().map(|c| (c.clone(), b.key_geometry(&c.id, mm))).collect()).unwrap_or_default(),
        typical,
        print: core.device_now()["print"].clone(),
    }
}

/// A rectangle of a picture, in its pixels: x, y, width, height.
type Rect = (usize, usize, usize, usize);

/// A free buffer of the pool for a picture `w` by `h` (made if none is
/// free), and what of it is out of date: this frame's changes, and those of
/// the frames drawn into the window's other buffers since it was last drawn
/// into (all of it, if it is new). Only that much is copied into it.
fn free_buffer(buffers: &mut Vec<(Buffer, Vec<Rect>)>, pool: &mut SlotPool, (w, h): (usize, usize), stride: usize, format: wl_shm::Format, dirty: &[Rect]) -> (usize, Vec<Rect>) {
    buffers.retain(|(b, _)| b.height() as usize == h && b.stride() as usize == stride);
    let i = match buffers.iter().position(|(b, _)| b.canvas(pool).is_some()) {
        Some(i) => i,
        None => {
            let (b, _) = pool.create_buffer(w as i32, h as i32, stride as i32, format).expect("a buffer");
            buffers.push((b, vec![(0, 0, w, h)]));
            buffers.len() - 1
        }
    };
    let mut todo = std::mem::take(&mut buffers[i].1);
    todo.extend_from_slice(dirty);
    for (j, (_, stale)) in buffers.iter_mut().enumerate() {
        if j != i {
            stale.extend_from_slice(dirty);
            if stale.len() > 32 {
                *stale = vec![(0, 0, w, h)];
            }
        }
    }
    (i, todo)
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
fn light_into(px: &mut Vec<f32>, board: &Board, lights: &[Light], locks: u8, ceiling: f32, reference: f64, panel: Option<&overlays::Panel>) {
    board.compose_into(px, lights, locks, reference as f32, ceiling);
    // The display, lit from behind as an LCD is: its lit dots, its unlit dots
    // and the ground between them each at the core's nits for them, in the
    // backlight's colour, under the glass. With nothing driving it, it is dark.
    if let Some(p) = panel {
        let (w, h) = (board.w as i32, board.h as i32);
        let lcd = board.lcd();
        let white = reference as f32;
        // the backlight is an LED: as much light as its colour has, white the most
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
                    p.dark[i].map(|v| v.max(0.0).powf(2.2))
                };
                let d = ((yy * w + x) * 4) as usize;
                px[d..d + 3].copy_from_slice(&e);
                px[d + 3] = 1.0;
            }
        }
    }
}
