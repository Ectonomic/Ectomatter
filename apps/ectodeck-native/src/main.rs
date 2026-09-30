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

mod device;
mod front;
mod overlays;
mod picture;
mod screens;
mod settings;
mod uistate;
mod z10;

slint::include_modules!();

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ectodeck_draw::hdr::{Board, Light};
use front::{Front, WindowAsk};
use picture::{Lit, Picture};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
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
/// the ground the keyboard sits on: the page's own
const GROUND: [u8; 3] = [0x20, 0x20, 0x20];
/// how far in from the window's edge a press resizes it
const EDGE: f64 = 6.0;

/// Slint, told to draw into our own buffer and to keep time by our clock.
struct Platform {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl slint::platform::Platform for Platform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
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
    let reference = best.and_then(|s| s["reference"].as_f64()).filter(|r| *r > 0.0).unwrap_or(203.0);
    let peak = settings.get("hdr.glow_limit").and_then(|v| v.as_f64()).or_else(|| best.and_then(|s| s["peak"].as_f64())).unwrap_or(reference);
    (reference, peak.max(reference))
}

/// Build the keyboard at each size asked for, away from the window's thread:
/// it takes a third of a second, and the window keeps drawing meanwhile.
fn board_builder(done: calloop::channel::Sender<Board>) -> std::sync::mpsc::Sender<(u32, u32)> {
    let (ask, asked) = std::sync::mpsc::channel::<(u32, u32)>();
    std::thread::spawn(move || {
        let tree = usvg::Tree::from_str(ectodeck_draw::z10::SVG, &usvg::Options::default()).expect("the Z10's art");
        let device: serde_json::Value = serde_json::from_str(ectodeck_draw::z10::DEVICE).expect("the Z10's device file");
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let snapshot = args.iter().position(|a| a == "--snapshot").and_then(|i| args.get(i + 1)).cloned();
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

    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(Platform { window: window.clone(), start: Instant::now() })).unwrap();
    let ui = AppWindow::new().unwrap();
    ui.set_mark(glyph(include_str!("../ui/glyphs/mark.svg"), 16, false));
    ui.set_home_glyph(glyph(include_str!("../ui/glyphs/home.svg"), 22, true));
    ui.set_keyboard_glyph(glyph(include_str!("../ui/glyphs/keyboard.svg"), 22, true));
    ui.set_settings_glyph(glyph(include_str!("../ui/glyphs/settings.svg"), 22, true));
    ui.set_keyboard_tile(glyph(include_str!("../ui/glyphs/keyboard.svg"), 88, true));
    ui.set_rail_open(uistate::get("rail_open").and_then(|v| v.as_bool()).unwrap_or(false));
    slint::ComponentHandle::show(&ui).unwrap();

    let settings = settings::Settings::load(config_dir().join("settings.json"));
    let (reference, glow) = levels(&settings);
    let gain = (glow / reference).max(1.0) as f32;
    eprintln!("ectodeck-native: the desktop's white is {reference} nits; the light goes to {glow}");

    if let Some(path) = snapshot {
        return snap(&window, ui, reference, gain, &path);
    }
    run(window, ui, &settings, reference, gain);
}

/// One frame without a window, saved for checking.
fn snap(window: &Rc<MinimalSoftwareWindow>, ui: AppWindow, reference: f64, gain: f32, path: &str) {
    let (w, h) = SIZE;
    window.set_size(PhysicalSize::new(w, h));
    let mut front = Front::new(ui, None, true);
    if let Some(state) = std::env::var_os("ECTODECK_NATIVE_STATE") {
        let dev: serde_json::Value = serde_json::from_slice(&std::fs::read(state).expect("the saved state")).expect("the saved state as JSON");
        front.heard(device::Heard::State(Ok(dev)));
    }
    if let Ok(place) = std::env::var("ECTODECK_NATIVE_PLACE") {
        front.ui.set_place(place.into());
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
    // keys typed on the keyboard, as Linux key codes: "29+" down, "29-" up
    for k in std::env::var("ECTODECK_NATIVE_EVDEV").unwrap_or_default().split(',').filter(|k| !k.is_empty()) {
        let (code, down) = (k.trim_end_matches(['+', '-']).parse().unwrap_or(0), k.ends_with('+'));
        front.typed(code, down);
        front.refresh();
    }
    // the pointer, where it was last left
    if let Ok(at) = std::env::var("ECTODECK_NATIVE_HOVER") {
        let mut xy = at.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        window.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(xy.next().unwrap_or(0.0), xy.next().unwrap_or(0.0)) });
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
    eprintln!("the hand shows: {}", front.ui.get_hand());
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
        let tree = usvg::Tree::from_str(ectodeck_draw::z10::SVG, &usvg::Options::default()).unwrap();
        let device: serde_json::Value = serde_json::from_str(ectodeck_draw::z10::DEVICE).unwrap();
        let t = Instant::now();
        let board = Board::new(&tree, &device, GROUND, rect.2 as u32, rect.3 as u32).unwrap();
        eprintln!("keyboard drawn in {:?}", t.elapsed());
        let lights = front.lights().unwrap_or_default();
        let lit = board.compose(&lights, gain);
        pic.set_keyboard(rect, Some(&Rc::new(Lit { px: lit, w: rect.2, h: rect.3 })));
    }
    let t = Instant::now();
    pic.encode_all();
    eprintln!("frame encoded in {:?}", t.elapsed());
    std::fs::write(path, bytemuck::cast_slice::<[u16; 4], u8>(&pic.out)).unwrap();
    // the white it was encoded for, which decoding it needs
    std::fs::write(format!("{path}.reference"), format!("{reference}\n")).unwrap();
    eprintln!("saved {path}: {w}x{h}, keyboard at {rect:?}");
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
    buffers: Vec<Buffer>,
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
    on_screen: Option<u32>,
    glow_limit: Option<f64>,
    levels: (f64, f64),

    slint: Rc<MinimalSoftwareWindow>,
    front: Front,
    pic: Picture,
    gain: f32,
    /// the keyboard at the size last drawn, and its light
    board: Option<Board>,
    lights: Vec<Light>,
    lit: Option<Rc<Lit>>,
    asked: Option<(u32, u32)>,
    build: std::sync::mpsc::Sender<(u32, u32)>,
    /// the whole window needs drawing again, not only what Slint changed
    full: bool,
    /// ECTODECK_NATIVE_DUMP: a folder to save the frames given to the compositor in
    dump: Option<PathBuf>,
    /// ECTODECK_NATIVE_CHECK: encode every frame whole too, and say if the
    /// parts encoded differ from it
    check: bool,
    frames: u64,
}

/// A screen's levels as KWin describes them: its white and its peak, in nits.
struct ScreenLevels {
    id: u32,
    _cmo: WpColorManagementOutputV1,
    reference: f64,
    peak: f64,
    reading: (f64, f64),
}

/// Which screen a description or its information was asked for.
#[derive(Clone, Copy)]
struct ScreenTag(u32);
/// A new description for the picture, for these levels (white, light's top).
#[derive(Clone, Copy)]
struct PictureTag(f64, f64);

/// The picture's description: PQ with BT.2020 primaries, its white at the desktop's.
fn describe<U: Send + Sync + 'static>(cm: &WpColorManagerV1, qh: &QueueHandle<App>, reference: f64, tag: U) -> WpImageDescriptionV1
where
    App: Dispatch<WpImageDescriptionV1, U>,
{
    let params = cm.create_parametric_creator(qh, ());
    params.set_tf_named(wp_color_manager_v1::TransferFunction::St2084Pq);
    params.set_primaries_named(wp_color_manager_v1::Primaries::Bt2020);
    params.set_luminances(0, 10000, reference.round() as u32);
    params.create(qh, tag)
}

fn run(slint: Rc<MinimalSoftwareWindow>, ui: AppWindow, settings: &settings::Settings, reference: f64, gain: f32) {
    let conn = Connection::connect_to_env().expect("a Wayland session");
    let (globals, queue) = registry_queue_init::<App>(&conn).unwrap();
    let qh = queue.handle();
    let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap();
    WaylandSource::new(conn.clone(), queue).insert(event_loop.handle()).unwrap();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let xdg = XdgShell::bind(&globals, &qh).expect("xdg_shell");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let cm: WpColorManagerV1 = globals.bind(&qh, 1..=1, ()).expect("the compositor's colour management (KWin 6.3 or later)");

    let surface = compositor.create_surface(&qh);
    let window = xdg.create_window(surface, WindowDecorations::RequestClient, &qh);
    window.set_title("Ectodeck");
    window.set_app_id("com.ectonomic.ectodeck");
    window.set_min_size(Some(MIN_SIZE));

    let desc = describe(&cm, &qh, reference, ());
    let cm_surface = cm.get_surface(window.wl_surface(), &qh, ());

    let (built_tx, built_rx) = calloop::channel::channel::<Board>();
    let build = board_builder(built_tx);
    event_loop
        .handle()
        .insert_source(built_rx, |e, _, app: &mut App| {
            if let calloop::channel::Event::Msg(board) = e {
                app.built(board);
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
        device::start(data_dir().join("backups"), heard_tx)
    });

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
        on_screen: None,
        glow_limit: settings.get("hdr.glow_limit").and_then(|v| v.as_f64()),
        levels: (reference, reference * gain as f64),
        slint,
        front: Front::new(ui, kb, controlled),
        pic: Picture::new(reference, GROUND),
        gain,
        board: None,
        lights: Vec::new(),
        lit: None,
        asked: None,
        build,
        full: true,
        dump: std::env::var_os("ECTODECK_NATIVE_DUMP").map(Into::into),
        check: std::env::var_os("ECTODECK_NATIVE_CHECK").is_some(),
        frames: 0,
    };

    // the picture's description has to be ready before the surface takes it
    while !app.ready && app.failed.is_none() {
        event_loop.dispatch(Some(Duration::from_millis(50)), &mut app).unwrap();
    }
    if let Some(e) = &app.failed {
        eprintln!("ectodeck-native: the compositor refused the HDR picture: {e}");
        return;
    }
    cm_surface.set_image_description(&desc, wp_color_manager_v1::RenderIntent::Perceptual);
    // mapped by a first commit with no buffer; the compositor answers with the size
    app.window.commit();

    while !app.exit {
        let busy = (app.slint.has_active_animations() || app.front.animated()) && !app.frame_pending;
        let wait = if busy {
            Some(Duration::ZERO)
        } else {
            [slint::platform::duration_until_next_timer_update(), app.front.waiting()].into_iter().flatten().min()
        };
        event_loop.dispatch(wait, &mut app).unwrap();
        slint::platform::update_timers_and_animations();
        app.front.retry();
        app.act();
        app.draw();
    }
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
                WindowAsk::Close => self.exit = true,
            }
        }
    }

    /// The screen the window is on has new levels: describe the picture for
    /// them. The new description takes over once KWin has it ready.
    fn retarget(&mut self, qh: &QueueHandle<Self>) {
        let Some(s) = self.screens.iter().find(|s| Some(s.id) == self.on_screen) else { return };
        if s.reference <= 0.0 {
            return;
        }
        let glow = self.glow_limit.unwrap_or(s.peak).max(s.reference);
        if (s.reference - self.levels.0).abs() < 0.5 && (glow - self.levels.1).abs() < 0.5 {
            return;
        }
        eprintln!("ectodeck-native: the screen's white is now {} nits; the light goes to {glow}", s.reference);
        describe(&self.cm, qh, s.reference, PictureTag(s.reference, glow));
    }

    /// The keyboard came back drawn at a size: light it.
    fn built(&mut self, board: Board) {
        let lit = board.compose(&self.lights, self.gain);
        self.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
        self.board = Some(board);
        self.full = true;
        self.draw();
    }

    fn draw(&mut self) {
        if !self.configured || self.frame_pending {
            return;
        }
        let (w, h) = ((self.size.0 * self.scale) as usize, (self.size.1 * self.scale) as usize);
        if self.pic.w != w || self.pic.h != h {
            self.pic.resize(w, h);
            self.full = true;
        }
        // the keys' light, when it has changed
        if let Some(lights) = self.front.lights() {
            self.lights = lights;
            if let Some(board) = &self.board {
                let lit = board.compose(&self.lights, self.gain);
                self.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
            }
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
            for (p, s) in region.iter() {
                dirty.push((p.x as usize, p.y as usize, s.width as usize, s.height as usize));
            }
        });

        if lay_ground(&mut self.pic, &self.front.ui, self.scale as f32) {
            self.full = true;
        }
        // the keyboard: the one drawn at this size, or the last one stretched to
        // it while this size is drawn
        let rect = board_rect(&self.front.ui, self.scale as f32);
        let want = (rect.2 as u32, rect.3 as u32);
        if want.0 > 0 && self.board.as_ref().map(|b| (b.w, b.h)) != Some(want) && self.asked != Some(want) {
            self.asked = Some(want);
            let _ = self.build.send(want);
        }
        self.front.board_size(rect.2 as u32, rect.3 as u32, self.scale as f32);
        let before = self.pic.keyboard_rect();
        if self.pic.set_keyboard(rect, self.lit.as_ref()) {
            for r in [before, rect] {
                dirty.push((r.0.max(0) as usize, r.1.max(0) as usize, r.2, r.3));
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
                let stale = parts.iter().zip(&self.pic.out).filter(|(a, b)| a != b).count();
                if stale > 0 {
                    eprintln!("ectodeck-native: CHECK frame {}: {stale} pixels differ from a whole encode", self.frames);
                }
            }
        }
        self.full = false;
        self.present(&dirty);
    }

    /// Hand the picture to the compositor.
    fn present(&mut self, dirty: &[(usize, usize, usize, usize)]) {
        let (w, h) = (self.pic.w, self.pic.h);
        let fits = |b: &Buffer| b.height() as usize == h && b.stride() as usize == w * 8;
        self.buffers.retain(fits);
        let pool = &mut self.pool;
        let free = self.buffers.iter().position(|b| b.canvas(pool).is_some());
        let i = match free {
            Some(i) => i,
            None => {
                let (b, _) = self.pool.create_buffer(w as i32, h as i32, (w * 8) as i32, wl_shm::Format::Abgr16161616).expect("a buffer");
                self.buffers.push(b);
                self.buffers.len() - 1
            }
        };
        let canvas = self.buffers[i].canvas(&mut self.pool).unwrap();
        canvas.copy_from_slice(bytemuck::cast_slice(&self.pic.out));
        let surface = self.window.wl_surface();
        for &(x, y, dw, dh) in dirty {
            surface.damage_buffer(x as i32, y as i32, dw as i32, dh as i32);
        }
        surface.set_buffer_scale(self.scale as i32);
        surface.frame(&self.qh, surface.clone());
        self.buffers[i].attach_to(surface).unwrap();
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
        if self.maximized {
            return None;
        }
        let (w, h) = (self.size.0 as f64, self.size.1 as f64);
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

    /// A key on the keyboard, pressed or let go.
    fn key(&mut self, e: &KeyEvent, down: bool) {
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
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, factor: i32) {
        self.scale = factor.max(1) as u32;
        self.slint.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: self.scale as f32 });
        self.slint.set_size(PhysicalSize::new(self.size.0 * self.scale, self.size.1 * self.scale));
        self.full = true;
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.frame_pending = false;
    }
    fn surface_enter(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_surface::WlSurface, output: &wl_output::WlOutput) {
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
        let desc = cmo.get_image_description(qh, ScreenTag(id));
        desc.get_information(qh, ScreenTag(id));
        self.screens.push(ScreenLevels { id, _cmo: cmo, reference: 0.0, peak: 0.0, reading: (0.0, 0.0) });
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window, configure: WindowConfigure, _: u32) {
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
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {
        self.slint.dispatch_event(WindowEvent::WindowActiveChanged(true));
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {
        self.slint.dispatch_event(WindowEvent::WindowActiveChanged(false));
    }
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(&event, true);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(&event, false);
    }
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: Modifiers, _: u32) {}
}

impl PointerHandler for App {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        const BTN_LEFT: u32 = 0x110;
        const BTN_RIGHT: u32 = 0x111;
        const BTN_MIDDLE: u32 = 0x112;
        for e in events {
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
                _ if self.front.ui.get_hand() => CursorIcon::Pointer,
                _ => CursorIcon::Default,
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
            app.gain = (glow / reference).max(1.0) as f32;
            if let Some(board) = &app.board {
                let lit = board.compose(&app.lights, app.gain);
                app.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
            }
            app.full = true;
            app.slint.request_redraw();
        }
    }
}

impl Dispatch<WpImageDescriptionV1, ScreenTag> for App {
    fn event(_: &mut Self, _: &WpImageDescriptionV1, _: wp_image_description_v1::Event, _: &ScreenTag, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WpColorManagementOutputV1, ScreenTag> for App {
    fn event(_: &mut Self, cmo: &WpColorManagementOutputV1, e: wp_color_management_output_v1::Event, tag: &ScreenTag, _: &Connection, qh: &QueueHandle<Self>) {
        if let wp_color_management_output_v1::Event::ImageDescriptionChanged = e {
            cmo.get_image_description(qh, *tag).get_information(qh, *tag);
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
                if app.on_screen == Some(tag.0) {
                    app.retarget(qh);
                }
            }
            _ => {}
        }
    }
}

delegate_noop!(App: ignore WpColorManagerV1);
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
