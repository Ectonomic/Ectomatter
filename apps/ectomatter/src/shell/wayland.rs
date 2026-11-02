//! The Wayland shell: Ectomatter's windows on a Wayland desktop, drawn into
//! shared memory and shown in HDR through the color-management protocol;
//! the person's pointer, keys, clipboard and drags; and the loop that wakes
//! for all of it and for what Ectomatter's threads tell the window. Moved
//! from main.rs as it was (2026-10-26); the frames it puts on the screen are
//! still composed here too.

use crate::*;
use kde_output::client::kde_output_device_mode_v2::KdeOutputDeviceModeV2;
use kde_output::client::kde_output_device_registry_v2::{self, KdeOutputDeviceRegistryV2};
use kde_output::client::kde_output_device_v2::{self, KdeOutputDeviceV2};
use smithay_client_toolkit::reexports::calloop::{self, EventLoop};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;
use smithay_client_toolkit::data_device_manager::{
    data_device::{DataDevice, DataDeviceHandler},
    data_offer::{receive_to_fd, DataOfferHandler, DragOffer},
    data_source::{CopyPasteSource, DataSourceHandler},
    DataDeviceManagerState, WritePipe,
};
use smithay_client_toolkit::reexports::client::protocol::{wl_data_device::WlDataDevice, wl_data_device_manager::DndAction, wl_data_source::WlDataSource};
use smithay_client_toolkit::{
    activation::{ActivationHandler, ActivationState, RequestData},
    compositor::{CompositorHandler, CompositorState},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{AxisScroll, CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer},
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

/// Where a press resizes a window: GRAB past its visible edge (the window's
/// surface is that much larger on every side, clear, and the compositor is
/// told where the window itself is, so its shadow and its placement go by
/// the window), and EDGE in from it; each corner takes CORNER of both edges
/// next to it, so a corner is as easy to find as an edge.
const GRAB: u32 = 10;
const EDGE: f64 = 3.0;
const CORNER: f64 = 20.0;

/// A window's clear margin round its visible edge, a side at a time, in
/// logical pixels: none while it is maximized or fills the screen, and none
/// on a side it is tiled against, where the margin would cover its
/// neighbor's edge.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Margins {
    l: u32,
    t: u32,
    r: u32,
    b: u32,
}

impl Margins {
    fn of(c: &WindowConfigure) -> Margins {
        if c.is_maximized() || c.is_fullscreen() {
            return Margins::default();
        }
        let m = |tiled: bool| if tiled { 0 } else { GRAB };
        Margins { l: m(c.is_tiled_left()), t: m(c.is_tiled_top()), r: m(c.is_tiled_right()), b: m(c.is_tiled_bottom()) }
    }
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
            // (with Ctrl held the key's text is a control character, Ctrl+C's
            // "\u{3}": the key's own letter in its place, which Slint's text
            // boxes take with Ctrl as copy, cut, paste and select all)
            let own = e.utf8.clone().filter(|t| !t.is_empty() && !t.chars().any(char::is_control));
            let t = own.or_else(|| e.keysym.key_char().map(|c| c.to_string()))?;
            return (!t.is_empty() && !t.chars().any(char::is_control)).then(|| t.into());
        }
    };
    Some(k.into())
}

use super::session::{self, Session};
use super::Rect;

pub(crate) struct App {
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
    /// KWin's shadow round each window, which Ectomatter's own title bars go without
    shadows: shadow::Shadows,
    buffers: Vec<(Buffer, Vec<Rect>)>,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    seat: Option<wl_seat::WlSeat>,
    /// the desktop's clipboard: its manager, the seat's device, the text
    /// Ectomatter offers on it, and the last key press's serial to offer it at
    data: Option<DataDeviceManagerState>,
    data_device: Option<DataDevice>,
    copy_source: Option<CopyPasteSource>,
    key_serial: u32,
    cursor: Option<CursorIcon>,
    /// the window's size in logical pixels, the clear margin round it, and the buffer's scale
    size: (u32, u32),
    margins: Margins,
    scale: u32,
    configured: bool,
    maximized: bool,
    /// whether the compositor says the person can't see the window: on another
    /// virtual desktop, or minimized (xdg-shell's suspended state)
    suspended: bool,
    frame_pending: bool,
    exit: bool,
    /// the last button press, which a move or a resize is started from
    press: u32,
    /// the compositor's window activation (none on one without it), with which a press in
    /// another of Ectomatter's windows brings the main one forward
    activation: Option<ActivationState>,
    at: (f64, f64),
    /// the wheel's steps running out over the window (shell/wheel.rs)
    wheel: super::wheel::Wheel,
    ready: bool,
    failed: Option<String>,
    /// color management: the picture's description on the surface, each
    /// screen's levels as KWin describes them, the screen the window is on,
    /// and the levels the picture is described for (the desktop's white, the screen's peak)
    cm: WpColorManagerV1,
    cm_surface: WpColorManagementSurfaceV1,
    screens: Vec<ScreenLevels>,
    /// what KDE says of each screen (its set SDR white), as its last word
    /// left it and as its words come in
    kde: Vec<(KdeOutputDeviceV2, kde_output::Device, kde_output::Device)>,
    _kde_registry: Option<KdeOutputDeviceRegistryV2>,
    on_screen: Option<u32>,
    /// the screens the window is on, in the order it reached them
    entered: Entered,
    levels: (f64, f64),
    /// the screen's own peak, for Settings › HDR
    levels_peak: f64,
    /// KWin takes the mastering display's luminance in a description
    mastering: bool,
    /// how KWin is asked to bring the picture to a screen or a screenshot: as
    /// drawn (relative, black to black) where it offers that. Perceptual let it
    /// squeeze the whole picture to fit the light into a screenshot's range,
    /// so red came out #ED0000 and white #BFBFBF.
    intent: wp_color_manager_v1::RenderIntent,
    /// closed into the tray: the window is unmapped, Ectomatter keeps running
    hidden: bool,
    /// the windows of their own beside the main one (the Macro Editor's, the
    /// lab's), once opened, and which of them has the keyboard's focus
    subs: [Option<SubWin>; 3],
    keys_on: Option<usize>,
    /// the keys held down as the keyboard last said, for Ctrl+Shift+K
    mods: Modifiers,
    /// what Ectomatter is doing, apart from the window (shell::session)
    s: Session,
}




/// A window of its own beside the main one: an ordinary SDR window, drawn whole by Slint.
pub(crate) struct SubWin {
    window: Window,
    slint: Rc<MinimalSoftwareWindow>,
    size: (u32, u32),
    margins: Margins,
    scale: u32,
    configured: bool,
    maximized: bool,
    frame_pending: bool,
    hidden: bool,
    full: bool,
    buffers: Vec<(Buffer, Vec<Rect>)>,
    px: Vec<PremultipliedRgbaColor>,
    at: (f64, f64),
    /// the wheel's steps running out over the window (shell/wheel.rs)
    wheel: super::wheel::Wheel,
    /// the pointer is over the window, and a button is held down on it
    inside: bool,
    held: bool,
}

/// A screen's levels as KWin describes them: its white and its peak, in nits.
pub(crate) struct ScreenLevels {
    id: u32,
    cmo: WpColorManagementOutputV1,
    reference: f64,
    peak: f64,
    reading: (f64, f64),
    /// the white it reported last while not dimmed (see kde_output::white)
    undimmed: Option<f64>,
}

/// The screens a window is on, in the order it reached them. Wayland says
/// which screens a window is on but not how much of it is on each, so, as
/// SDL does, the window counts as on the last one it reached that it's
/// still on (and, off every screen, as when closed to the tray, on none).
#[derive(Default)]
pub(crate) struct Entered(Vec<u32>);

impl Entered {
    /// The window reached a screen: the one it's on now.
    fn enter(&mut self, id: u32) -> u32 {
        self.0.retain(|&s| s != id);
        self.0.push(id);
        id
    }
    /// The window left a screen: the one it's on now, if it's on any.
    fn leave(&mut self, id: u32) -> Option<u32> {
        self.0.retain(|&s| s != id);
        self.0.last().copied()
    }
}

#[cfg(test)]
mod entered_tests {
    use super::Entered;

    /// The window counts as on the last screen it reached that it's still
    /// on: moved partly onto a second screen and back, it's on the first
    /// again, not left on the second.
    #[test]
    fn the_window_is_on_the_last_screen_it_reached_that_it_is_still_on() {
        let mut e = Entered::default();
        assert_eq!(e.enter(1), 1);
        assert_eq!(e.enter(2), 2, "reaching a second screen, across both");
        assert_eq!(e.leave(2), Some(1), "back on the first alone");
        assert_eq!(e.enter(2), 2);
        assert_eq!(e.leave(1), Some(2), "moved over to the second");
        assert_eq!(e.enter(2), 2, "a screen reached twice is listed once");
        assert_eq!(e.leave(2), None, "on no screen, as when closed to the tray");
        assert_eq!(e.leave(3), None, "a screen it was never on");
    }
}

/// Which screen a description or its information was asked for.
#[derive(Clone, Copy)]
pub(crate) struct ScreenTag(u32);
/// A new description for the picture, for these levels (the white, the screen's peak).
#[derive(Clone, Copy)]
pub(crate) struct PictureTag(f64, f64);

/// The picture's description: PQ with sRGB's primaries (BT.709, the colors
/// it is drawn in), its white at the desktop's, and its brightest light no
/// brighter than the screen's peak. Without that last, KWin takes the picture
/// to reach PQ's 10,000 nits and, fitting it perceptually (where it offers
/// nothing better: see App::intent), tone-maps it down to the screen, which
/// dims every light well below the nits it was drawn at.
pub(crate) fn describe<U: Send + Sync + 'static>(cm: &WpColorManagerV1, qh: &QueueHandle<App>, reference: f64, peak: f64, mastering: bool, tag: U) -> WpImageDescriptionV1
where
    App: smithay_client_toolkit::reexports::client::Dispatch<WpImageDescriptionV1, U>,
{
    let params = cm.create_parametric_creator(qh, ());
    params.set_tf_named(wp_color_manager_v1::TransferFunction::St2084Pq);
    params.set_primaries_named(wp_color_manager_v1::Primaries::Srgb);
    params.set_luminances(0, 10000, reference.round() as u32);
    let peak = peak.max(reference + 1.0).round() as u32;
    if mastering {
        params.set_mastering_luminance(1, peak);
    }
    params.set_max_cll(peak);
    params.set_max_fall(peak);
    params.create(qh, tag)
}

/// What wakes the loop that's running now: the one waiting for a compositor,
/// or the window's, each with a ping of its own.
type Slot = std::sync::Arc<std::sync::Mutex<Option<calloop::ping::Ping>>>;

/// Ectomatter at work from the start, with its window on the compositor
/// whenever there's one to show it on: none yet at login, when Ectomatter can
/// start before KWin is ready (as on 2026-10-28), and none for a while when
/// KWin goes away and starts again. Without one, Ectomatter goes on as with
/// its window closed into the tray (`wait`).
pub(crate) fn run(slint: Rc<MinimalSoftwareWindow>, made: Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>, ui: AppWindow, settings: settings::Settings, reference: f64, gain: f32) {
    // Ectomatter's threads tell it through senders that ping the loop running now
    let slot: Slot = Default::default();
    let wake: shell::Wake = {
        let slot = slot.clone();
        std::sync::Arc::new(move || {
            if let Some(p) = slot.lock().ok().as_deref().and_then(Option::as_ref) {
                p.ping();
            }
        })
    };
    let mut s = Session::start(slint, made, ui, settings, reference, gain, &wake);
    // `--hide`: started into the tray, as at login
    let mut shown = !std::env::args().any(|a| a == "--hide");
    let mut now = true;
    while let Some(conn) = wait(&mut s, &slot, &mut shown, now) {
        let (back, lost) = window(s, conn, &slot, shown);
        s = back;
        match lost {
            Some(was) => (shown, now) = (was, false),
            None => break,
        }
    }
    s.finish();
}

/// Ectomatter with no compositor to show its window on: its devices, packages,
/// tray and the app in front go on as with the window closed into the tray, and
/// a compositor is looked for each second (at once where `now`). The tray's
/// Open, meanwhile, has the window shown once there's one (`shown`). The
/// compositor's connection; none once Ectomatter is to stop.
fn wait(s: &mut Session, slot: &Slot, shown: &mut bool, now: bool) -> Option<Connection> {
    if now && let Ok(c) = Connection::connect_to_env() {
        return Some(c);
    }
    eprintln!("ectomatter: no compositor yet; Ectomatter works on without its window and shows it once there's one");
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        // (as Plasma's login starts programs on Fedora KDE 44 before KWin has told
        // systemd where it is: the login entry waits for KWin, autostart.rs)
        eprintln!("ectomatter: it was started without WAYLAND_DISPLAY, so no compositor will be found");
    }
    let mut event_loop: EventLoop<()> = EventLoop::try_new().expect("a loop to wait for the compositor in");
    let (ping, ping_source) = calloop::ping::make_ping().expect("a ping for the waiting loop");
    event_loop.handle().insert_source(ping_source, |_, _, _| {}).expect("the waiting loop takes what Ectomatter's threads tell it");
    if let Ok(mut p) = slot.lock() {
        *p = Some(ping);
    }
    s.front.set_hidden(true);
    let mut looked = Instant::now();
    loop {
        if looked.elapsed() >= Duration::from_secs(1) {
            looked = Instant::now();
            if let Ok(c) = Connection::connect_to_env() {
                eprintln!("ectomatter: the compositor is there");
                return Some(c);
            }
        }
        if let session::Step::Stop = s.step() {
            return None;
        }
        for a in s.take(1.0) {
            match a {
                tray::TrayAsk::Open(_) => {
                    *shown = true;
                    s.front.open_fresh();
                }
                tray::TrayAsk::Device(d) => {
                    *shown = true;
                    s.front.open_fresh();
                    s.front.open_at_first_page(&d);
                }
                tray::TrayAsk::Scene(_) | tray::TrayAsk::Quit => {}
            }
        }
        // (what the page asks of its window comes of a press on it: none here)
        let _ = s.front.act();
        // (nothing drawn: the light and the tray's menus kept, as with the window closed)
        s.compose(1.0, None, s.front.screen_peak, s.front.levels.1);
        slint::platform::update_timers_and_animations();
        let next = Duration::from_secs(1).saturating_sub(looked.elapsed());
        let wait = [Some(next), slint::platform::duration_until_next_timer_update(), s.front.waiting()].into_iter().flatten().min();
        let _ = event_loop.dispatch(wait, &mut ());
    }
}

/// Ectomatter's window on the compositor `conn` reaches, shown or closed into
/// the tray (`shown`), until Ectomatter is to stop (none) or the compositor
/// goes away (whether the window was showing then).
fn window(s: Session, conn: Connection, slot: &Slot, shown: bool) -> (Session, Option<bool>) {
    let Ok((globals, queue)) = registry_queue_init::<App>(&conn) else { return (s, Some(shown)) };
    let qh = queue.handle();
    let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap();
    WaylandSource::new(conn.clone(), queue).insert(event_loop.handle()).unwrap();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let xdg = XdgShell::bind(&globals, &qh).expect("xdg_shell");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let cm: WpColorManagerV1 = globals.bind(&qh, 1..=1, ()).expect("the compositor's color management (KWin 6.3 or later)");
    // KDE's account of each screen, for the SDR white its user set (none on another compositor)
    let kde_registry: Option<KdeOutputDeviceRegistryV2> = globals.bind(&qh, 21..=23, ()).ok();
    let shadow_manager: Option<shadow::client::org_kde_kwin_shadow_manager::OrgKdeKwinShadowManager> = globals.bind(&qh, 1..=2, ()).ok();

    let surface = compositor.create_surface(&qh);
    let window = xdg.create_window(surface, WindowDecorations::RequestClient, &qh);
    name(&window, "Ectomatter", MIN_SIZE);

    let reference = s.pic.reference();
    let desc = describe(&cm, &qh, reference, screens::screens().iter().filter_map(|sc| sc["peak"].as_f64()).fold(reference, f64::max), false, ());
    let cm_surface = cm.get_surface(window.wl_surface(), &qh, ());

    // Ectomatter's threads tell the window through senders that ping this loop
    let (ping, ping_source) = calloop::ping::make_ping().expect("a ping for the window's loop");
    if let Ok(mut p) = slot.lock() {
        *p = Some(ping);
    }
    let mut app = App {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        pool: SlotPool::new((SIZE.0 * SIZE.1 * 8) as usize, &shm).expect("a shared memory pool"),
        shadows: shadow::Shadows::new(shadow_manager),
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
        data: DataDeviceManagerState::bind(&globals, &qh).ok(),
        data_device: None,
        copy_source: None,
        key_serial: 0,
        cursor: None,
        size: SIZE,
        margins: Margins::default(),
        scale: 1,
        configured: false,
        maximized: false,
        suspended: false,
        frame_pending: false,
        exit: false,
        press: 0,
        activation: ActivationState::bind(&globals, &qh).ok(),
        at: (0.0, 0.0),
        wheel: Default::default(),
        ready: false,
        failed: None,
        cm,
        cm_surface: cm_surface.clone(),
        screens: Vec::new(),
        kde: Vec::new(),
        _kde_registry: kde_registry,
        on_screen: None,
        entered: Entered::default(),
        levels_peak: 1000.0,
        mastering: false,
        intent: wp_color_manager_v1::RenderIntent::Perceptual,
        levels: (0.0, 0.0),
        hidden: false,
        subs: [None, None, None],
        keys_on: None,
        mods: Modifiers::default(),
        s,
    };
    event_loop
        .handle()
        .insert_source(ping_source, |_, _, app: &mut App| app.take())
        .expect("the window's loop takes what Ectomatter's threads tell it");
    // started into the tray (`--hide`, as at login), or closed into it when the
    // compositor went away: the window stays closed (with no tray to come back
    // from, it shows: see the loop)
    if shown {
        app.s.front.set_hidden(false);
    } else {
        app.unmap();
    }

    // the picture's description has to be ready before the surface takes it
    while !app.ready && app.failed.is_none() {
        if event_loop.dispatch(Some(Duration::from_millis(50)), &mut app).is_err() {
            return app.lost();
        }
    }
    if let Some(e) = app.failed.take() {
        eprintln!("ectomatter: the compositor refused the HDR picture: {e}");
        return (app.s, None);
    }
    cm_surface.set_image_description(&desc, app.intent);
    eprintln!("ectomatter: the compositor {} the mastering display's luminance", if app.mastering { "takes" } else { "does not take" });
    eprintln!("ectomatter: the compositor {}", if app.intent == wp_color_manager_v1::RenderIntent::RelativeBpc { "shows the picture's colors as drawn" } else { "fits the picture's colors to each screen (perceptual)" });
    // mapped by a first commit with no buffer; the compositor answers with the size
    app.window.commit();

    let mut trial = std::env::var_os("ECTOMATTER_TRAYTEST").map(|_| Instant::now());
    let mut fps = std::env::var_os("ECTOMATTER_FPS").map(|_| (Instant::now(), 0u32, Duration::ZERO));
    // ECTOMATTER_OPEN_MACROS=N: open the Macro Editor on macro N once the keyboard is read, to check it
    let mut open_macros = std::env::var("ECTOMATTER_OPEN_MACROS").ok().and_then(|v| v.parse::<u32>().ok());
    // ECTOMATTER_OPEN_LAB: open the lab at start, as Ctrl+Shift+K does
    let mut open_lab = std::env::var_os("ECTOMATTER_OPEN_LAB").is_some();
    while !app.exit {
        if let session::Step::Stop = app.s.step() {
            app.exit = true;
            break;
        }
        let busy = ((app.s.slint.has_active_animations() || app.wheel.running()) && !app.frame_pending)
            || app.subs.iter().flatten().any(|m| !m.hidden && (m.slint.has_active_animations() || m.wheel.running()) && !m.frame_pending);
        // a lighting effect's next frame, 30 a second; none while the window is
        // closed into the tray, where it is not drawn
        let effect = (app.s.front.animated() && !app.frame_pending && !app.hidden).then(|| app.s.effect_due.saturating_duration_since(Instant::now()));
        let wait = if busy {
            Some(Duration::ZERO)
        } else {
            [slint::platform::duration_until_next_timer_update(), app.s.front.waiting(), effect].into_iter().flatten().min()
        };
        if event_loop.dispatch(wait, &mut app).is_err() {
            return app.lost();
        }
        app.wheel_due();
        slint::platform::update_timers_and_animations();
        // closed into a tray that isn't there (none on this desktop, or gone a
        // while): shown, as there's no other way back to it
        if app.hidden && !app.s.tray_there() {
            app.show();
        }
        // ECTOMATTER_TRAYTEST: close into the tray once, three seconds in, to check the way back
        if let Some(t) = trial {
            if t.elapsed() > Duration::from_secs(3) {
                trial = None;
                eprintln!("ectomatter: test: closing into the tray");
                app.hide();
            }
        }
        // (Ectomatter's own macros don't wait for the keyboard)
        if let Some(n) = open_macros.filter(|_| macro_lib() == macros::Lib::Ectomatter || app.s.front.page.dev.is_some()) {
            open_macros = None;
            app.open(session::Open::Macros(macro_lib(), n));
        }
        if open_lab && app.s.board.is_some() {
            open_lab = false;
            app.open(session::Open::Lab);
        }
        app.act();
        let t = Instant::now();
        let before = app.s.frames;
        app.draw();
        app.draw_sub(MACROS);
        app.draw_sub(LAB);
        app.draw_sub(LAYOUTS);
        // ECTOMATTER_FPS: frames a second and the slowest, to check smoothness without slowing it
        if let Some((since, n, slow)) = &mut fps {
            if app.s.frames > before {
                *n += 1;
                *slow = (*slow).max(t.elapsed());
            }
            if since.elapsed() >= Duration::from_secs(1) {
                if *n > 0 {
                    eprintln!("ectomatter: {} frames a second, the slowest drawn in {:?}", n, slow);
                }
                (*since, *n, *slow) = (Instant::now(), 0, Duration::ZERO);
            }
        }

    }
    (app.s, None)
}

/// Whether a scroll comes from a wheel, whose steps are eased (shell/wheel.rs),
/// rather than a touchpad's or a knob's run, smooth already: its source says,
/// or without one (an older compositor), its counted clicks.
fn a_wheel(source: Option<wl_pointer::AxisSource>, h: &AxisScroll, v: &AxisScroll) -> bool {
    match source {
        Some(wl_pointer::AxisSource::Wheel | wl_pointer::AxisSource::WheelTilt) => true,
        Some(_) => false,
        None => h.discrete != 0 || v.discrete != 0 || h.value120 != 0 || v.value120 != 0,
    }
}

impl App {
    /// What the wheel's steps have due over each window, scrolled where the
    /// pointer is.
    fn wheel_due(&mut self) {
        let now = Instant::now();
        if let Some(d) = self.wheel.due(now) {
            let position = LogicalPosition::new(self.at.0 as f32, self.at.1 as f32);
            self.s.slint.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: d[0], delta_y: d[1] });
        }
        for mw in self.subs.iter_mut().flatten() {
            if let Some(d) = mw.wheel.due(now) {
                let position = LogicalPosition::new(mw.at.0 as f32, mw.at.1 as f32);
                mw.slint.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: d[0], delta_y: d[1] });
            }
        }
    }

    /// What Ectomatter's threads have told the window, taken in; and what
    /// the tray asks of the window itself.
    fn take(&mut self) {
        for a in self.s.take(self.scale as f32) {
            match a {
                tray::TrayAsk::Open(token) => self.come_forward(token),
                tray::TrayAsk::Device(d) => {
                    self.come_forward(None);
                    self.s.front.open_at_first_page(&d);
                }
                tray::TrayAsk::Scene(_) | tray::TrayAsk::Quit => {}
            }
        }
    }

    /// Draw the window's next frame, if it can take one now.
    fn draw(&mut self) {
        let size = (!self.hidden && self.configured && !self.frame_pending).then(|| ((self.size.0 * self.scale) as usize, (self.size.1 * self.scale) as usize));
        let top = self.top_peak();
        if let Some(dirty) = self.s.compose(self.scale as f32, size, self.levels_peak, top) {
            self.present(&dirty);
        }
    }

    /// Do what the page asked.
    fn act(&mut self) {
        let asks = self.s.front.act();
        for ask in &asks {
            match ask {
                WindowAsk::Move if !asks.contains(&WindowAsk::Maximize) => {
                    if let Some(seat) = &self.seat {
                        self.window.move_(seat, self.press);
                    }
                    // the compositor keeps the button's release: tell the page it is up
                    let position = LogicalPosition::new(self.at.0 as f32, self.at.1 as f32);
                    self.s.slint.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
                    self.s.slint.dispatch_event(WindowEvent::PointerExited);
                }
                WindowAsk::Move => {}
                WindowAsk::Minimize => self.window.set_minimized(),
                WindowAsk::Maximize => {
                    if self.maximized {
                        self.window.unset_maximized()
                    } else {
                        self.window.set_maximized()
                    }
                }
                WindowAsk::Close => self.hide(),
                WindowAsk::OpenMacros(lib, n) => self.open(session::Open::Macros(*lib, *n)),
                WindowAsk::MacroPlays(n, how) => self.s.front.macro_plays(*n, *how),
                WindowAsk::MacroGone(n) => self.s.front.macro_gone(*n),
                WindowAsk::MacroSaved(n) => self.s.front.macro_saved(*n),
                WindowAsk::OpenLab => self.open(session::Open::Lab),
                WindowAsk::OpenLayouts(i) => self.open(session::Open::Layouts(*i)),
                WindowAsk::ShowSettings => {
                    self.show_settings(None);
                    self.s.front.show_sensors();
                }
            }
        }
        let asks = [(MACROS, self.s.front.act_macros()), (LAB, self.s.front.act_lab()), (LAYOUTS, self.s.front.act_layouts())];
        // the editor's window changed under a resting pointer (a block dropped or deleted
        // where it is): what is under the pointer now shows its hover and its cursor. Not
        // while a button is held, as a drag's own moves keep it up to date then
        if self.s.front.layouts_rehover() {
            if let Some(sw) = self.subs[LAYOUTS].as_ref().filter(|sw| sw.inside && !sw.held && !sw.hidden) {
                sw.slint.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(sw.at.0 as f32, sw.at.1 as f32) });
            }
        }
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
                        (sw.held, sw.inside) = (false, false);
                    }
                    WindowAsk::Minimize => sw.window.set_minimized(),
                    WindowAsk::Maximize => {
                        if sw.maximized {
                            sw.window.unset_maximized()
                        } else {
                            sw.window.set_maximized()
                        }
                    }
                    WindowAsk::Close => self.hide_sub(which),
                    WindowAsk::ShowSettings => {
                        let from = sw.window.wl_surface().clone();
                        self.show_settings(Some(from));
                        self.s.front.show_sensors();
                    }
                    // (what the Macro Editor's changes mean for the mouse's buttons)
                    WindowAsk::MacroPlays(n, how) => self.s.front.macro_plays(n, how),
                    WindowAsk::MacroGone(n) => self.s.front.macro_gone(n),
                    WindowAsk::MacroSaved(n) => self.s.front.macro_saved(n),
                    WindowAsk::OpenMacros(..) | WindowAsk::OpenLab | WindowAsk::OpenLayouts(_) => {}
                }
            }
        }
    }

    /// A window of its own beside the main one, for the Slint window made last.
    fn make_sub(&mut self, slint: Rc<MinimalSoftwareWindow>, title: &str, size: (u32, u32), min: (u32, u32)) -> SubWin {
        let surface = self.compositor.create_surface(&self.qh);
        let window = self._xdg.create_window(surface, WindowDecorations::RequestClient, &self.qh);
        name(&window, title, min);
        window.commit();
        SubWin { window, slint, size, margins: Margins::default(), scale: 1, configured: false, maximized: false, frame_pending: false, hidden: false, full: true, buffers: Vec::new(), px: Vec::new(), at: (0.0, 0.0), wheel: Default::default(), inside: false, held: false }
    }

    /// Show a window of its own again, as it was.
    fn show_sub(&mut self, which: usize) {
        if let Some(sw) = &mut self.subs[which] {
            if sw.hidden {
                sw.hidden = false;
                sw.full = true;
                if let Some((title, _, min)) = &self.s.shapes[which] {
                    name(&sw.window, title, *min);
                }
                sw.window.commit();
            }
        }
    }

    /// A window of its own opened: made the first time, shown again after.
    fn open(&mut self, what: session::Open) {
        match self.s.open(what) {
            Some((which, session::Opened::New { window, title, size, min })) => self.subs[which] = Some(self.make_sub(window, &title, size, min)),
            // (its window gone with a compositor that went away: made again, as it was)
            Some((which, session::Opened::Again)) if self.subs[which].is_none() => {
                if let (Some(w), Some((title, size, min))) = (self.s.windows[which].clone(), self.s.shapes[which].clone()) {
                    self.subs[which] = Some(self.make_sub(w, &title, size, min));
                }
            }
            Some((which, session::Opened::Again)) => self.show_sub(which),
            None => {}
        }
    }



    /// Close a window of its own; it is kept, to open again as it was. The
    /// Macro Editor's lets a recording go.
    fn hide_sub(&mut self, which: usize) {
        self.s.closed(which);
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
        let s = mw.scale as usize;
        let m = mw.margins;
        let (ml, mt) = (m.l as usize * s, m.t as usize * s);
        let (bw, bh) = (w + ml + m.r as usize * s, h + mt + m.b as usize * s);
        let (i, todo, fresh) = free_buffer(&mut mw.buffers, &mut self.pool, (w, h), (bw, bh), bw * 4, wl_shm::Format::Argb8888, &changed);
        let canvas = mw.buffers[i].0.canvas(&mut self.pool).unwrap();
        if fresh {
            canvas.fill(0);
        }
        // the compositor's ARGB, which little-endian memory holds as B, G, R, A;
        // only what is out of date in this buffer, inside the window's clear margin
        for &(x, y, rw, rh) in &todo {
            let (x1, y1) = ((x + rw).min(w), (y + rh).min(h));
            for row in y.min(y1)..y1 {
                let (a, b) = (row * w + x.min(x1), row * w + x1);
                let at = (row + mt) * bw + ml + x.min(x1);
                for (d, p) in canvas[at * 4..(at + b - a) * 4].chunks_exact_mut(4).zip(&mw.px[a..b]) {
                    d.copy_from_slice(&[p.blue, p.green, p.red, p.alpha]);
                }
            }
        }
        let surface = mw.window.wl_surface();
        if fresh {
            surface.damage_buffer(0, 0, bw as i32, bh as i32);
        }
        surface.damage_buffer(dx + ml as i32, dy + mt as i32, dw, dh);
        surface.set_buffer_scale(mw.scale as i32);
        surface.frame(&self.qh, smithay_client_toolkit::compositor::FrameCallbackData(surface.clone()));
        mw.buffers[i].0.attach_to(surface).unwrap();
        mw.window.commit();
        mw.frame_pending = true;
        if let Some(dir) = &self.s.dump {
            let rgba: Vec<u8> = mw.px.iter().flat_map(|p| [p.red, p.green, p.blue, p.alpha]).collect();
            let name = ["macros", "lab", "layouts"][which];
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
        // (the surface's corner is the margin's; the window's own starts past it)
        let (sx, sy) = e.position;
        let (x, y) = (sx - mw.margins.l as f64, sy - mw.margins.t as f64);
        mw.at = (x, y);
        let position = LogicalPosition::new(x as f32, y as f32);
        match e.kind {
            PointerEventKind::Enter { .. } => {
                self.cursor = None;
                mw.inside = true;
                mw.slint.dispatch_event(WindowEvent::PointerMoved { position });
            }
            PointerEventKind::Leave { .. } => {
                mw.inside = false;
                mw.slint.dispatch_event(WindowEvent::PointerExited);
            }
            PointerEventKind::Motion { .. } => mw.slint.dispatch_event(WindowEvent::PointerMoved { position }),
            PointerEventKind::Press { button: b, serial, .. } => {
                self.press = serial;
                mw.held = true;
                if b == 0x110 {
                    if let (Some(edge), Some(seat)) = (edge(mw.size, mw.margins, mw.maximized, sx, sy), &self.seat) {
                        mw.window.resize(seat, serial, edge);
                        return;
                    }
                }
                self.s.pressed_in(which, x as f32, y as f32);
                mw.slint.dispatch_event(WindowEvent::PointerPressed { position, button: pointer_button(b) });
            }
            PointerEventKind::Release { button: b, .. } => {
                mw.held = false;
                mw.slint.dispatch_event(WindowEvent::PointerReleased { position, button: pointer_button(b) });
                // (Slint asks for the cursor again only when the pointer next moves: told
                // now, the hand that was dragging shows what is under it the moment it lets go)
                mw.slint.dispatch_event(WindowEvent::PointerMoved { position });
            }
            PointerEventKind::Axis { horizontal, vertical, source, .. } => {
                let d = [-horizontal.absolute as f32, -vertical.absolute as f32];
                if a_wheel(source, &horizontal, &vertical) {
                    mw.wheel.step(d, Instant::now());
                } else {
                    mw.slint.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: d[0], delta_y: d[1] });
                }
            }
        }
        let icon = edge_cursor(edge(mw.size, mw.margins, mw.maximized, sx, sy)).unwrap_or(mw.slint.pointer());
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
        if (white - self.levels.0).abs() < 0.5 && (self.levels_peak - self.levels.1).abs() < 0.5 {
            return;
        }
        // (as bright as the keys' light can go here, as Settings › HDR lets it)
        let glow = front::gain_for(&self.s.front.settings.get("hdr").unwrap_or_default(), white, self.levels_peak) as f64 * white;
        eprintln!("ectomatter: the screen's white is now {white} nits; the light goes to {glow:.0}");
        describe(&self.cm, qh, white, self.levels_peak, self.mastering, PictureTag(white, self.levels_peak));
    }

    /// The brightest screen's peak: the top of Settings › HDR's Peak nits
    /// wherever the window is, so a dimmer screen never lowers the setting.
    fn top_peak(&self) -> f64 {
        self.screens.iter().map(|s| s.peak).fold(self.levels_peak, f64::max)
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

    /// The main window on Ectomatter's Settings, shown again if it was closed to the tray, and
    /// brought forward with the press just made in another of Ectomatter's windows (`from`),
    /// as the compositor lets a window be raised only with a press.
    fn show_settings(&mut self, from: Option<wl_surface::WlSurface>) {
        self.s.front.ui.set_place("settings".into());
        self.s.front.refresh();
        self.show();
        self.s.full = true;
        if let (Some(a), Some(seat), Some(surface)) = (&self.activation, &self.seat, from) {
            a.request_token(&self.qh, RequestData { app_id: Some("com.ectonomic.ectomatter".into()), seat_and_serial: Some((seat.clone(), self.press)), surface: Some(surface), udata: () });
        }
    }

    /// Close the window into the tray: without its buffer the window is
    /// unmapped, and Ectomatter carries on. With no tray, closing quits.
    fn hide(&mut self) {
        self.s.front.hand_back();
        // (a scene being set, unkept: the scene on plays again)
        self.s.front.cancel_scene_form();
        if !self.s.tray_there() {
            self.s.quit();
            return;
        }
        self.unmap();
    }

    /// The window unmapped (without its buffer), and Ectomatter carrying on.
    fn unmap(&mut self) {
        self.window.wl_surface().attach(None, 0, 0);
        self.window.commit();
        self.hidden = true;
        // (out of sight, the screen plays again rather than show the keyboard's own)
        self.s.front.set_hidden(true);
        self.configured = false;
        self.frame_pending = false;
        self.buffers.clear();
    }

    /// The compositor gone: each window of its own closed as though the person
    /// had closed it (its page kept, its window made again when it's opened),
    /// what the window had borrowed handed back, and Ectomatter on without its
    /// window until there's a compositor again; whether the window was showing.
    fn lost(self) -> (Session, Option<bool>) {
        eprintln!("ectomatter: the compositor went away; Ectomatter works on without its window until there's one again");
        let shown = !self.hidden;
        let mut s = self.s;
        for (which, sub) in self.subs.iter().enumerate() {
            if sub.as_ref().is_some_and(|w| !w.hidden) {
                s.closed(which);
            }
        }
        s.front.hand_back();
        s.front.cancel_scene_form();
        (s, Some(shown))
    }

    /// The window brought to the person (the tray's icon pressed, or Ectomatter
    /// opened again): shown, if it was closed into the tray; made again, if the
    /// person can't see it (on another virtual desktop, or minimized), as a window
    /// just opened goes on the desktop and the screen they're on; and raised with
    /// the press's activation token, where there is one. A window opened, or opened
    /// again, starts at Home (first_page.rs); one the person can see is left as it is.
    fn come_forward(&mut self, token: Option<String>) {
        match forward(self.hidden, self.suspended) {
            Forward::Show => {
                self.s.front.open_fresh();
                self.show();
            }
            Forward::Again => {
                eprintln!("ectomatter: the window was out of sight, so it's opened again where the person is");
                self.s.front.open_fresh();
                self.unmap();
                self.show();
            }
            Forward::Raise => {}
        }
        if let (Some(a), Some(token)) = (&self.activation, token) {
            a.activate::<App>(self.window.wl_surface(), token);
        }
    }

    /// Bring the window back: a first commit with no buffer, as when it was
    /// made, and the compositor answers with its size.
    fn show(&mut self) {
        if !self.hidden {
            return;
        }
        eprintln!("ectomatter: shown again from the tray");
        self.hidden = false;
        self.suspended = false;
        self.s.front.set_hidden(false);
        self.s.full = true;
        name(&self.window, "Ectomatter", MIN_SIZE);
        self.window.commit();
        // (one hidden before it was first shown, as at login, has its size already: the
        // compositor sends nothing new, so it's drawn now)
        if self.configured {
            self.draw();
        }
    }

    /// Hand the picture to the compositor, inside the window's clear margin.
    fn present(&mut self, dirty: &[(usize, usize, usize, usize)]) {
        let (w, h) = (self.s.pic.w, self.s.pic.h);
        let s = self.scale as usize;
        let m = self.margins;
        let (ml, mt) = (m.l as usize * s, m.t as usize * s);
        let (bw, bh) = (w + ml + m.r as usize * s, h + mt + m.b as usize * s);
        let (i, todo, fresh) = free_buffer(&mut self.buffers, &mut self.pool, (w, h), (bw, bh), bw * 8, wl_shm::Format::Abgr16161616, dirty);
        let canvas = self.buffers[i].0.canvas(&mut self.pool).unwrap();
        if fresh {
            canvas.fill(0);
        }
        let src: &[u8] = bytemuck::cast_slice(&self.s.pic.out);
        for &(x, y, rw, rh) in &todo {
            let (x1, y1) = ((x + rw).min(w), (y + rh).min(h));
            for row in y.min(y1)..y1 {
                let (a, b) = ((row * w + x.min(x1)) * 8, (row * w + x1) * 8);
                let at = ((row + mt) * bw + ml + x.min(x1)) * 8;
                canvas[at..at + (b - a)].copy_from_slice(&src[a..b]);
            }
        }
        let surface = self.window.wl_surface();
        if fresh {
            surface.damage_buffer(0, 0, bw as i32, bh as i32);
        }
        // (many small boxes, a ripple's dots, told as the one box round them)
        let merged = (dirty.len() > 32).then(|| bounding(dirty));
        for &(x, y, dw, dh) in merged.as_ref().map_or(dirty, std::slice::from_ref) {
            surface.damage_buffer((x + ml) as i32, (y + mt) as i32, dw as i32, dh as i32);
        }
        surface.set_buffer_scale(self.scale as i32);
        surface.frame(&self.qh, smithay_client_toolkit::compositor::FrameCallbackData(surface.clone()));
        self.buffers[i].0.attach_to(surface).unwrap();
        self.window.commit();
        self.frame_pending = true;
        self.s.frames += 1;
        if let Some(dir) = &self.s.dump {
            let _ = std::fs::write(dir.join("frame.raw"), bytemuck::cast_slice::<[u16; 4], u8>(&self.s.pic.out));
            let _ = std::fs::write(dir.join("frame.txt"), format!("{} {} {} {}\n", w, h, self.s.frames, self.s.pic.reference()));
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

    /// The window's edge under the pointer at `x`, `y` on its surface, which a press resizes from.
    fn edge(&self, x: f64, y: f64) -> Option<ResizeEdge> {
        edge(self.size, self.margins, self.maximized, x, y)
    }

    /// A key on the keyboard, pressed or let go.
    fn key(&mut self, e: &KeyEvent, down: bool) {
        let p = shell::Press { text: key_text(e), code: e.raw_code, down, ctrl: self.mods.ctrl, shift: self.mods.shift };
        if self.s.key_in(self.keys_on, p) {
            self.open(session::Open::Lab);
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
            // the lab's legends drawn again at the screen's own pixels
            if which == LAB {
                if let Some(l) = &mut self.s.front.lab {
                    l.refresh();
                }
            }
            return;
        }
        self.scale = factor.max(1) as u32;
        self.s.slint.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: self.scale as f32 });
        self.s.slint.set_size(PhysicalSize::new(self.size.0 * self.scale, self.size.1 * self.scale));
        self.s.full = true;
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
        self.on_screen = Some(self.entered.enter(output.id().protocol_id()));
        self.retarget(qh);
    }
    fn surface_leave(&mut self, _: &Connection, qh: &QueueHandle<Self>, surface: &wl_surface::WlSurface, output: &wl_output::WlOutput) {
        if surface != self.window.wl_surface() {
            return;
        }
        use smithay_client_toolkit::reexports::client::Proxy;
        // back on the screen it's still on (off every screen, it keeps the last one's levels)
        if let Some(id) = self.entered.leave(output.id().protocol_id()) {
            self.on_screen = Some(id);
            self.retarget(qh);
        }
    }
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
    fn output_destroyed(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        use smithay_client_toolkit::reexports::client::Proxy;
        // a screen unplugged or turned off: its levels go with it (so Peak nits
        // tops out at a screen that's still there)
        let id = output.id().protocol_id();
        if let Some(i) = self.screens.iter().position(|s| s.id == id) {
            self.screens.remove(i).cmo.destroy();
        }
        if let Some(now) = self.entered.leave(id) {
            self.on_screen = Some(now);
            self.retarget(qh);
        }
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(which) = self.which_sub(window.wl_surface()) {
            self.hide_sub(which);
        } else {
            self.hide();
        }
    }

    fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, window: &Window, configure: WindowConfigure, _: u32) {
        // its shadow, but when it fills the screen
        self.shadows.set(window.wl_surface(), !configure.is_maximized(), &mut self.pool, qh);
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
            // (a new margin: every buffer drawn anew, its margin cleared)
            let m = Margins::of(&configure);
            if m != mw.margins {
                mw.margins = m;
                mw.buffers.clear();
                mw.full = true;
            }
            window.xdg_surface().set_window_geometry(m.l as i32, m.t as i32, w as i32, h as i32);
            mw.configured = true;
            // the page laid out again for the window's width: the macro's chain wraps at it
            if which == MACROS {
                if let Some(m) = &mut self.s.front.macros {
                    m.ui.set_is_maximized(maximized);
                    m.refresh(&self.s.front.page);
                }
            } else if which == LAYOUTS {
                if let Some(l) = &mut self.s.front.layouts {
                    l.ui.set_is_maximized(maximized);
                }
                self.s.front.show_layouts();
            } else if let Some(l) = &mut self.s.front.lab {
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
            self.s.slint.set_size(PhysicalSize::new(w * self.scale, h * self.scale));
            self.s.full = true;
        }
        self.suspended = configure.state.contains(smithay_client_toolkit::reexports::csd_frame::WindowState::SUSPENDED);
        let maximized = configure.is_maximized();
        if maximized != self.maximized {
            self.maximized = maximized;
            self.s.front.ui.set_is_maximized(maximized);
        }
        // (a new margin: every buffer drawn anew, its margin cleared)
        let m = Margins::of(&configure);
        if m != self.margins {
            self.margins = m;
            self.buffers.clear();
            self.s.full = true;
        }
        window.xdg_surface().set_window_geometry(m.l as i32, m.t as i32, w as i32, h as i32);
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
            self.pointer = self.seats.get_pointer_with_theme::<_, ()>(qh, &seat, self.shm.wl_shm(), surface, ThemeSpec::default()).ok();
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
            self.data_device = self.data.as_ref().map(|d| d.get_data_device(qh, &seat));
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
            None => self.s.slint.dispatch_event(WindowEvent::WindowActiveChanged(true)),
        }
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface, _: u32) {
        match self.which_sub(surface).and_then(|w| self.subs[w].as_ref()) {
            Some(sw) => sw.slint.dispatch_event(WindowEvent::WindowActiveChanged(false)),
            None => self.s.slint.dispatch_event(WindowEvent::WindowActiveChanged(false)),
        }
        self.keys_on = None;
    }
    fn press_key(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, serial: u32, event: KeyEvent) {
        self.key_serial = serial;
        self.key(&event, true);
        // text the key copied or cut, offered on the desktop's clipboard
        if let (Some(text), Some(data), Some(device)) = (clipboard::take_pending(), &self.data, &self.data_device) {
            let source = data.create_copy_paste_source(qh, clipboard::TEXT);
            source.set_selection(device, serial);
            clipboard::hold(Some(text));
            self.copy_source = Some(source);
        }
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(&event, false);
    }
    // (a key held repeats on the compositor's word only where it says so, wl_keyboard 10; the
    // window has never repeated a held key, so it takes the first press alone, as before)
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, mods: Modifiers, _: RawModifiers, _: u32) {
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
            // (the surface's corner is the margin's; the window's own starts past it)
            let (sx, sy) = e.position;
            let (x, y) = (sx - self.margins.l as f64, sy - self.margins.t as f64);
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
                    self.s.slint.dispatch_event(WindowEvent::PointerMoved { position });
                }
                PointerEventKind::Leave { .. } => {
                    self.s.slint.dispatch_event(WindowEvent::PointerExited);
                }
                PointerEventKind::Motion { .. } => {
                    self.s.slint.dispatch_event(WindowEvent::PointerMoved { position });
                }
                PointerEventKind::Press { button: b, serial, .. } => {
                    self.press = serial;
                    if b == BTN_LEFT {
                        if let (Some(edge), Some(seat)) = (self.edge(sx, sy), &self.seat) {
                            self.window.resize(seat, serial, edge);
                            continue;
                        }
                    }
                    self.s.front.pressed_at(x as f32, y as f32);
                    self.s.slint.dispatch_event(WindowEvent::PointerPressed { position, button: button(b) });
                }
                PointerEventKind::Release { button: b, .. } => {
                    self.s.slint.dispatch_event(WindowEvent::PointerReleased { position, button: button(b) });
                    // (the cursor asked for again at once: see sub_pointer)
                    self.s.slint.dispatch_event(WindowEvent::PointerMoved { position });
                }
                PointerEventKind::Axis { horizontal, vertical, source, .. } => {
                    let d = [-horizontal.absolute as f32, -vertical.absolute as f32];
                    if a_wheel(source, &horizontal, &vertical) {
                        self.wheel.step(d, Instant::now());
                    } else {
                        self.s.slint.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: d[0], delta_y: d[1] });
                    }
                }
            }
            // the cursor: resizing at the edges, the hand over what can be clicked
            let icon = match self.edge(sx, sy) {
                Some(ResizeEdge::TopLeft) => CursorIcon::NwResize,
                Some(ResizeEdge::TopRight) => CursorIcon::NeResize,
                Some(ResizeEdge::BottomLeft) => CursorIcon::SwResize,
                Some(ResizeEdge::BottomRight) => CursorIcon::SeResize,
                Some(ResizeEdge::Left) => CursorIcon::WResize,
                Some(ResizeEdge::Right) => CursorIcon::EResize,
                Some(ResizeEdge::Top) => CursorIcon::NResize,
                Some(ResizeEdge::Bottom) => CursorIcon::SResize,
                _ => self.s.slint.pointer(),
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

/// A protocol whose events Ectomatter has no use for, the objects made with no data.
macro_rules! ignore {
    ($proto:ty) => {
        impl Dispatch2<$proto, App> for () {
            fn event(&self, _: &mut App, _: &$proto, _: <$proto as smithay_client_toolkit::reexports::client::Proxy>::Event, _: &Connection, _: &QueueHandle<App>) {}
        }
    };
}

impl Dispatch2<WpImageDescriptionV1, App> for () {
    fn event(&self, app: &mut App, _: &WpImageDescriptionV1, e: wp_image_description_v1::Event, _: &Connection, _: &QueueHandle<App>) {
        match e {
            wp_image_description_v1::Event::Ready { .. } | wp_image_description_v1::Event::Ready2 { .. } => app.ready = true,
            wp_image_description_v1::Event::Failed { msg, .. } => app.failed = Some(msg),
            _ => {}
        }
    }
}

impl Dispatch2<WpImageDescriptionV1, App> for PictureTag {
    fn event(&self, app: &mut App, desc: &WpImageDescriptionV1, e: wp_image_description_v1::Event, _: &Connection, _: &QueueHandle<App>) {
        if let wp_image_description_v1::Event::Ready { .. } | wp_image_description_v1::Event::Ready2 { .. } = e {
            // the description and the pixels drawn for it go to KWin together, on the next frame
            let PictureTag(reference, peak) = *self;
            app.cm_surface.set_image_description(desc, app.intent);
            app.levels = (reference, peak);
            app.s.pic.set_reference(reference);
            app.s.gain = LIGHT_GAIN;
            if let Some(board) = &app.s.board {
                let lit = light(board, &app.s.lights, app.s.front.page.locks, app.s.gain, app.s.pic.reference(), app.s.front.overlays.panel_for(board.w, board.lcd().dot));
                app.s.lit = Some(Rc::new(Lit { px: lit, w: board.w as usize, h: board.h as usize }));
            }
            app.s.full = true;
            app.s.slint.request_redraw();
        }
    }
}

/// A screen's description, asked for its levels once KWin has it ready: a
/// screen turned off has none, and asking a failed description for its
/// levels is a protocol error, which ends the window's connection.
impl Dispatch2<WpImageDescriptionV1, App> for ScreenTag {
    fn event(&self, _: &mut App, desc: &WpImageDescriptionV1, e: wp_image_description_v1::Event, _: &Connection, qh: &QueueHandle<App>) {
        match e {
            wp_image_description_v1::Event::Ready { .. } | wp_image_description_v1::Event::Ready2 { .. } => {
                desc.get_information(qh, *self);
                desc.destroy();
            }
            wp_image_description_v1::Event::Failed { .. } => desc.destroy(),
            _ => {}
        }
    }
}

impl Dispatch2<WpColorManagementOutputV1, App> for ScreenTag {
    fn event(&self, _: &mut App, cmo: &WpColorManagementOutputV1, e: wp_color_management_output_v1::Event, _: &Connection, qh: &QueueHandle<App>) {
        if let wp_color_management_output_v1::Event::ImageDescriptionChanged = e {
            cmo.get_image_description(qh, *self);
        }
    }
}

impl Dispatch2<WpImageDescriptionInfoV1, App> for ScreenTag {
    fn event(&self, app: &mut App, _: &WpImageDescriptionInfoV1, e: wp_image_description_info_v1::Event, _: &Connection, qh: &QueueHandle<App>) {
        let Some(s) = app.screens.iter_mut().find(|s| s.id == self.0) else { return };
        match e {
            wp_image_description_info_v1::Event::Luminances { max_lum, reference_lum, .. } => s.reading = (reference_lum as f64, max_lum as f64),
            wp_image_description_info_v1::Event::Done => {
                (s.reference, s.peak) = s.reading;
                // its white undimmed, for while it is dimmed
                let id = self.0;
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

impl Dispatch2<shadow::client::org_kde_kwin_shadow_manager::OrgKdeKwinShadowManager, App> for () {
    fn event(&self, _: &mut App, _: &shadow::client::org_kde_kwin_shadow_manager::OrgKdeKwinShadowManager, _: shadow::client::org_kde_kwin_shadow_manager::Event, _: &Connection, _: &QueueHandle<App>) {}
}

impl Dispatch2<shadow::client::org_kde_kwin_shadow::OrgKdeKwinShadow, App> for () {
    fn event(&self, _: &mut App, _: &shadow::client::org_kde_kwin_shadow::OrgKdeKwinShadow, _: shadow::client::org_kde_kwin_shadow::Event, _: &Connection, _: &QueueHandle<App>) {}
}

impl Dispatch2<KdeOutputDeviceRegistryV2, App> for () {
    fn event(&self, app: &mut App, _: &KdeOutputDeviceRegistryV2, e: kde_output_device_registry_v2::Event, _: &Connection, _: &QueueHandle<App>) {
        if let kde_output_device_registry_v2::Event::Output { output } = e {
            app.kde.push((output, kde_output::Device::default(), kde_output::Device::default()));
        }
    }
    smithay_client_toolkit::reexports::client::event_created_child!(App, KdeOutputDeviceRegistryV2, [
        kde_output_device_registry_v2::EVT_OUTPUT_OPCODE => (KdeOutputDeviceV2, ()),
    ]);
}

impl Dispatch2<KdeOutputDeviceV2, App> for () {
    fn event(&self, app: &mut App, dev: &KdeOutputDeviceV2, e: kde_output_device_v2::Event, _: &Connection, qh: &QueueHandle<App>) {
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

ignore!(KdeOutputDeviceModeV2);

impl Dispatch2<WpColorManagerV1, App> for () {
    fn event(&self, app: &mut App, _: &WpColorManagerV1, e: wp_color_manager_v1::Event, _: &Connection, _: &QueueHandle<App>) {
        use smithay_client_toolkit::reexports::client::WEnum;
        match e {
            wp_color_manager_v1::Event::SupportedFeature { feature: WEnum::Value(wp_color_manager_v1::Feature::SetMasteringDisplayPrimaries) } => app.mastering = true,
            wp_color_manager_v1::Event::SupportedIntent { render_intent: WEnum::Value(wp_color_manager_v1::RenderIntent::RelativeBpc) } => app.intent = wp_color_manager_v1::RenderIntent::RelativeBpc,
            _ => {}
        }
    }
}
ignore!(WpColorManagementSurfaceV1);
ignore!(WpImageDescriptionCreatorParamsV1);

impl DataDeviceHandler for App {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64, _: &wl_surface::WlSurface) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    /// What is on the clipboard changed: another program's text is read in
    /// the background (Ectomatter's own it pastes from what it holds).
    fn selection(&mut self, conn: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        let Some(offer) = self.data_device.as_ref().and_then(|d| d.data().selection_offer()) else {
            clipboard::read_theirs(None);
            return;
        };
        let Some(mime) = offer.with_mime_types(|m| clipboard::TEXT.iter().find(|t| m.iter().any(|o| o == *t)).map(|t| t.to_string())) else {
            clipboard::read_theirs(None);
            return;
        };
        match std::io::pipe() {
            Ok((read, write)) => {
                receive_to_fd(offer.inner(), mime, write.into());
                let _ = conn.flush();
                clipboard::read_theirs(Some(read));
            }
            Err(_) => clipboard::read_theirs(None),
        }
    }
}

impl DataOfferHandler for App {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

impl DataSourceHandler for App {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}
    /// A program pastes Ectomatter's text: written to it, the pipe then closed.
    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: String, mut fd: WritePipe) {
        if let Some(text) = clipboard::ours() {
            use std::io::Write;
            let _ = fd.write_all(text.as_bytes());
        }
    }
    /// Another program holds the clipboard now.
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        // (dropped, the source is destroyed)
        if self.copy_source.as_ref().is_some_and(|s| s.inner() == source) {
            self.copy_source = None;
            clipboard::hold(None);
        }
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}
delegate_registry!(App);
smithay_client_toolkit::delegate_dispatch2!(App);

impl ActivationHandler for App {
    type RequestUdata = ();
    /// The token a press in another window was given: the main window comes forward with it.
    fn new_token(&mut self, token: String, _: &RequestData<()>) {
        if let Some(a) = &self.activation {
            a.activate::<App>(self.window.wl_surface(), token);
        }
    }
}

/// A window's title, its app (the desktop entry the taskbar shows it by) and
/// its smallest size, told the compositor as it's made, and again each time it's
/// shown after it was closed: unmapped, a window is made again from nothing, and
/// KWin forgets all three.
fn name(window: &Window, title: &str, min: (u32, u32)) {
    window.set_title(title);
    window.set_app_id("com.ectonomic.ectomatter");
    window.set_min_size(Some(min));
}

/// What bringing the window to the person takes: showing it, closed into the
/// tray (`hidden`); making it again, where the compositor says the person can't
/// see it (`suspended`), so it opens where they are; or only raising it, where
/// they can.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Forward {
    Show,
    Again,
    Raise,
}

fn forward(hidden: bool, suspended: bool) -> Forward {
    if hidden {
        Forward::Show
    } else if suspended {
        Forward::Again
    } else {
        Forward::Raise
    }
}

/// A window's edge under the pointer at `x`, `y` on its surface (its clear
/// margin `m` round the window), which a press resizes from: anywhere in the
/// margin, or EDGE in from the window's edge; a corner along CORNER of each
/// edge from it.
pub(crate) fn edge(size: (u32, u32), m: Margins, maximized: bool, x: f64, y: f64) -> Option<ResizeEdge> {
    if maximized {
        return None;
    }
    let (w, h) = (size.0 as f64, size.1 as f64);
    let (x, y) = (x - m.l as f64, y - m.t as f64);
    let (l, r, t, b) = (x < EDGE, x > w - EDGE, y < EDGE, y > h - EDGE);
    let (l, r) = (l || ((t || b) && x < CORNER), r || ((t || b) && x > w - CORNER));
    let (t, b) = (t || ((l || r) && y < CORNER), b || ((l || r) && y > h - CORNER));
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

pub(crate) fn pointer_button(b: u32) -> PointerEventButton {
    match b {
        0x110 => PointerEventButton::Left,
        0x111 => PointerEventButton::Right,
        0x112 => PointerEventButton::Middle,
        _ => PointerEventButton::Other,
    }
}

/// The cursor for resizing from an edge.
pub(crate) fn edge_cursor(e: Option<ResizeEdge>) -> Option<CursorIcon> {
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



/// A rectangle of a picture, in its pixels: x, y, width, height.

/// A free buffer of the pool `bw` by `bh` for a picture `w` by `h` inside
/// the window's margin (made if none is free), what of the picture in it is
/// out of date: this frame's changes, and those of the frames drawn into the
/// window's other buffers since it was last drawn into (all of it, if it is
/// new), which is all that is copied into it; and whether it is new, its
/// margin still to be cleared.
pub(crate) fn free_buffer(buffers: &mut Vec<(Buffer, Vec<Rect>)>, pool: &mut SlotPool, (w, h): (usize, usize), (bw, bh): (usize, usize), stride: usize, format: wl_shm::Format, dirty: &[Rect]) -> (usize, Vec<Rect>, bool) {
    buffers.retain(|(b, _)| b.height() as usize == bh && b.stride() as usize == stride);
    let (i, fresh) = match buffers.iter().position(|(b, _)| b.canvas(pool).is_some()) {
        Some(i) => (i, false),
        None => {
            let (b, _) = pool.create_buffer(bw as i32, bh as i32, stride as i32, format).expect("a buffer");
            buffers.push((b, vec![(0, 0, w, h)]));
            (buffers.len() - 1, true)
        }
    };
    let mut todo = std::mem::take(&mut buffers[i].1);
    todo.extend_from_slice(dirty);
    for (j, (_, stale)) in buffers.iter_mut().enumerate() {
        if j != i {
            stale.extend_from_slice(dirty);
            if stale.len() > 32 {
                *stale = vec![bounding(stale)];
            }
        }
    }
    (i, todo, fresh)
}

/// The one box round all of `boxes`.
pub(crate) fn bounding(boxes: &[Rect]) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for &(x, y, w, h) in boxes {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + w), y1.max(y + h));
    }
    if x0 > x1 {
        return (0, 0, 0, 0);
    }
    (x0, y0, x1 - x0, y1.saturating_sub(y0))
}




#[cfg(test)]
mod edge_tests {
    use super::*;

    /// The window's edges, past it and just inside it; its corners along
    /// both edges next to them; none while maximized, and no margin on a
    /// tiled side.
    #[test]
    fn a_window_resizes_from_round_its_edge() {
        let m = Margins { l: GRAB, t: GRAB, r: GRAB, b: GRAB };
        let at = |x: f64, y: f64| edge((1440, 1028), m, false, x, y);
        // (the surface's corner is 10 above and left of the window's)
        assert_eq!(at(2.0, 500.0), Some(ResizeEdge::Left), "in the margin, outside the window");
        assert_eq!(at(11.0, 500.0), Some(ResizeEdge::Left), "just inside the window's edge");
        assert_eq!(at(14.0, 500.0), None, "the window itself");
        assert_eq!(at(1455.0, 500.0), Some(ResizeEdge::Right));
        assert_eq!(at(700.0, 1045.0), Some(ResizeEdge::Bottom));
        // a corner along both edges next to it, not only where they meet
        assert_eq!(at(25.0, 3.0), Some(ResizeEdge::TopLeft), "on the top edge, near the left");
        assert_eq!(at(4.0, 26.0), Some(ResizeEdge::TopLeft), "on the left edge, near the top");
        assert_eq!(at(45.0, 3.0), Some(ResizeEdge::Top), "further along the top edge");
        assert_eq!(at(1440.0, 1040.0), Some(ResizeEdge::BottomRight));
        assert_eq!(edge((1440, 1028), m, true, 2.0, 500.0), None, "maximized");
        let tiled = Margins { l: 0, ..m };
        assert_eq!(edge((1440, 1028), tiled, false, 2.0, 500.0), Some(ResizeEdge::Left), "tiled: the window's own edge");
        assert_eq!(edge((1440, 1028), tiled, false, 5.0, 500.0), None);
    }
}


#[cfg(test)]
mod key_tests {
    use super::*;

    /// A letter with Ctrl held reaches the text boxes as the letter, so
    /// Ctrl+C copies; typed alone, as typed; a control character never.
    #[test]
    fn ctrl_c_reaches_the_text_boxes_as_c() {
        let key = |keysym, utf8: Option<&str>| key_text(&KeyEvent { time: 0, raw_code: 46, keysym, utf8: utf8.map(str::to_string) });
        assert_eq!(key(Keysym::c, Some("\u{3}")).as_deref(), Some("c"), "Ctrl+C");
        assert_eq!(key(Keysym::x, Some("\u{18}")).as_deref(), Some("x"), "Ctrl+X");
        assert_eq!(key(Keysym::c, Some("c")).as_deref(), Some("c"), "c");
        assert_eq!(key(Keysym::C, Some("C")).as_deref(), Some("C"), "Shift+C");
        assert_eq!(key(Keysym::c, None).as_deref(), Some("c"), "c let go");
    }

    /// A wheel's steps are eased; a touchpad's run, and a knob's, go as
    /// they come; without a source, a scroll with counted clicks is a wheel's.
    #[test]
    fn a_wheel_is_told_from_a_touchpad() {
        use wl_pointer::AxisSource;
        let (none, mut click) = (AxisScroll::default(), AxisScroll::default());
        click.value120 = 120;
        assert!(a_wheel(Some(AxisSource::Wheel), &none, &click) && a_wheel(Some(AxisSource::WheelTilt), &click, &none));
        assert!(!a_wheel(Some(AxisSource::Finger), &none, &none) && !a_wheel(Some(AxisSource::Continuous), &none, &none));
        assert!(a_wheel(None, &none, &click) && !a_wheel(None, &none, &none));
    }
}
