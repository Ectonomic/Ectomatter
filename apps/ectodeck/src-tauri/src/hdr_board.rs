//! The keyboard in true HDR, drawn by Ectodeck itself under a see-through
//! part of the webview.
//!
//! WebKitGTK only draws SDR, so anything brighter than the desktop's white has
//! to come from a surface of Ectodeck's own: one Wayland sub-surface BELOW the
//! window's own surface, on GTK's own connection (the only way to relate it to
//! it), at the keyboard's place on the page. It carries one picture: the whole
//! keyboard, flattened by hdr_draw from the Z10 core's art, with each key's
//! light filled in and going brighter than the desktop's white. The page paints
//! the ground round the keyboard itself, see-through over the keyboard only,
//! and draws what is interactive or live on top (html.hdr in style.css).
//!
//! The picture is declared PQ with BT.2020 primaries at the screen's reference
//! white. The keyboard is drawn once for its size; each change of light fills
//! the keys' shapes in again (hdr_draw's compose) and hands KWin a new picture.

use serde::Deserialize;

/// Where things are, in the window's pixels, as the page reports them.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Layout {
    pub win_w: u32,
    pub win_h: u32,
    /// the window's corner radius; 0 when maximized
    pub radius: f64,
    /// the ground's color, as the page's CSS has it (#rrggbb)
    pub base: String,
    /// the board's rect, or none when the keyboard page isn't showing
    pub board: Option<[f64; 4]>,
    /// what sent this layout, for ECTODECK_HDR_TEST's log
    #[serde(default)]
    pub why: String,
    /// how bright the light may go, in nits (0: no brighter than the desktop's white)
    #[serde(default)]
    pub glow: f64,
    /// each lit key's light: position, red, green, blue, strength (0 to 1), remapped (0 or 1)
    #[serde(default)]
    pub lights: Option<Vec<f64>>,
}

/// The page's lights, six numbers a key, as the HDR layer takes them.
pub fn lights(flat: &[f64]) -> Vec<crate::hdr_draw::Light> {
    flat.chunks_exact(6)
        .map(|c| crate::hdr_draw::Light {
            pos: c[0] as u32,
            rgb: [c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8, c[3].clamp(0.0, 255.0) as u8],
            strength: c[4].clamp(0.0, 1.0) as f32,
            dim: c[5] > 0.5,
        })
        .collect()
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Layout;
    use std::io::Write;
    use std::os::fd::AsFd;
    use std::sync::mpsc;
    use std::sync::Mutex;
    use wayland_backend::client::{Backend, ObjectId};
    use wayland_client::protocol::{wl_buffer, wl_compositor, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface};
    use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
    use wayland_protocols::wp::color_management::v1::client::{
        wp_color_management_surface_v1, wp_color_manager_v1, wp_image_description_creator_params_v1, wp_image_description_v1,
    };

    /// The running layer: where to send it layouts, and which showing of the
    /// window it is built on.
    static CHANNEL: Mutex<Option<(mpsc::Sender<Msg>, u64)>> = Mutex::new(None);

    /// What the page sends the layer: where things are, or the keys' light.
    pub enum Msg {
        Layout(Layout),
        Light(Vec<crate::hdr_draw::Light>),
    }

    /// The keys' light, for the running layer (none running: nothing to light).
    pub fn light(lights: Vec<crate::hdr_draw::Light>) {
        if let Ok(ch) = CHANNEL.lock() {
            if let Some((tx, _)) = ch.as_ref() {
                let _ = tx.send(Msg::Light(lights));
            }
        }
    }
    /// How many times the window has been shown. Each showing has a new
    /// surface, which Wayland may give the old one's id and GTK its address,
    /// so the count is what tells them apart.
    static SHOWINGS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static COUNTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// Show the layout, starting the HDR layer the first time. Only the main
    /// window has the layer, and a window with no size (hidden, or not laid out
    /// yet) has nothing to show: a buffer of no size is a protocol error, which
    /// ends the whole app.
    pub fn layout(window: &tauri::WebviewWindow, layout: Layout, reference: f64, art: (String, String)) -> Result<(), String> {
        if window.label() != "main" || layout.win_w == 0 || layout.win_h == 0 {
            return Ok(());
        }
        // The window's surface as it is now. GTK makes a new one each time the
        // window is shown again (closing it only hides it, to the tray), and a
        // hidden window has none, so there is nothing to draw on until it shows.
        let Ok((display, surface, showing)) = gtk_pointers(window) else { return Ok(()) };
        let mut ch = CHANNEL.lock().map_err(|e| e.to_string())?;
        if let Some((tx, on)) = ch.as_ref() {
            if *on == showing && tx.send(Msg::Layout(layout.clone())).is_ok() {
                return Ok(());
            }
        }
        // a new surface, or none yet: the old layer, if any, ends with its own
        // (dropping its sender ends its thread), and one starts on this one
        *ch = None;
        let (tx, rx) = mpsc::channel::<Msg>();
        tx.send(Msg::Layout(layout)).map_err(|e| e.to_string())?;
        let key = showing;
        std::thread::Builder::new()
            .name("hdr-board".into())
            .spawn(move || {
                if let Err(e) = run(display, surface, reference, art, rx) {
                    eprintln!("ectodeck: HDR board: {e}");
                }
                // clear the channel only if it is still this layer's
                if let Ok(mut c) = CHANNEL.lock() {
                    if c.as_ref().is_some_and(|(_, on)| *on == key) {
                        *c = None;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        *ch = Some((tx, key));
        Ok(())
    }

    /// GTK's wl_display, the window's wl_surface and which showing of the window
    /// it belongs to, fetched on the main thread.
    fn gtk_pointers(window: &tauri::WebviewWindow) -> Result<(usize, usize, u64), String> {
        use gtk::glib::translate::ToGlibPtr;
        use gtk::prelude::*;
        unsafe extern "C" {
            fn gdk_wayland_window_get_wl_surface(window: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            fn gdk_wayland_display_get_wl_display(display: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        }
        let (tx, rx) = mpsc::channel::<Result<(usize, usize, u64), String>>();
        let win = window.clone();
        window
            .run_on_main_thread(move || {
                let r = (|| {
                    let gtk_window = win.gtk_window().map_err(|e| e.to_string())?;
                    // count the window's showings from now on (it is showing now)
                    use std::sync::atomic::Ordering;
                    if !COUNTING.swap(true, Ordering::SeqCst) {
                        SHOWINGS.fetch_add(1, Ordering::SeqCst);
                        gtk_window.connect_map(|_| {
                            SHOWINGS.fetch_add(1, Ordering::SeqCst);
                        });
                    }
                    let gdk_window = gtk_window.window().ok_or("the window is not realized")?;
                    let display = gdk_window.display();
                    let d: *mut gtk::gdk::ffi::GdkDisplay = display.to_glib_none().0;
                    let s: *mut gtk::gdk::ffi::GdkWindow = gdk_window.to_glib_none().0;
                    let wl_display = unsafe { gdk_wayland_display_get_wl_display(d as *mut _) };
                    let wl_surface = unsafe { gdk_wayland_window_get_wl_surface(s as *mut _) };
                    if wl_display.is_null() || wl_surface.is_null() {
                        return Err("no Wayland surface".to_string());
                    }
                    Ok((wl_display as usize, wl_surface as usize, SHOWINGS.load(Ordering::SeqCst)))
                })();
                let _ = tx.send(r);
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    #[derive(Default)]
    struct St {
        compositor: Option<wl_compositor::WlCompositor>,
        sub: Option<wl_subcompositor::WlSubcompositor>,
        shm: Option<wl_shm::WlShm>,
        cm: Option<wp_color_manager_v1::WpColorManagerV1>,
        ready: bool,
        failed: Option<String>,
    }
    impl Dispatch<wl_registry::WlRegistry, ()> for St {
        fn event(st: &mut Self, reg: &wl_registry::WlRegistry, e: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
            if let wl_registry::Event::Global { name, interface, version } = e {
                match interface.as_str() {
                    "wl_compositor" => st.compositor = Some(reg.bind(name, version.min(4), qh, ())),
                    "wl_subcompositor" => st.sub = Some(reg.bind(name, 1, qh, ())),
                    "wl_shm" => st.shm = Some(reg.bind(name, 1, qh, ())),
                    "wp_color_manager_v1" => st.cm = Some(reg.bind(name, 1, qh, ())),
                    _ => {}
                }
            }
        }
    }
    impl Dispatch<wp_image_description_v1::WpImageDescriptionV1, ()> for St {
        fn event(st: &mut Self, _: &wp_image_description_v1::WpImageDescriptionV1, e: wp_image_description_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            match e {
                wp_image_description_v1::Event::Ready { .. } | wp_image_description_v1::Event::Ready2 { .. } => st.ready = true,
                wp_image_description_v1::Event::Failed { msg, .. } => st.failed = Some(msg),
                _ => {}
            }
        }
    }
    delegate_noop!(St: ignore wl_compositor::WlCompositor);
    delegate_noop!(St: ignore wl_subcompositor::WlSubcompositor);
    delegate_noop!(St: ignore wl_subsurface::WlSubsurface);
    delegate_noop!(St: ignore wl_surface::WlSurface);
    delegate_noop!(St: ignore wl_region::WlRegion);
    delegate_noop!(St: ignore wl_shm::WlShm);
    delegate_noop!(St: ignore wl_shm_pool::WlShmPool);
    delegate_noop!(St: ignore wl_buffer::WlBuffer);
    delegate_noop!(St: ignore wp_color_manager_v1::WpColorManagerV1);
    delegate_noop!(St: ignore wp_color_management_surface_v1::WpColorManagementSurfaceV1);
    delegate_noop!(St: ignore wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1);

    fn hex(s: &str) -> [u8; 3] {
        let s = s.trim().trim_start_matches('#');
        let v = |i: usize| u8::from_str_radix(s.get(i..i + 2).unwrap_or("20"), 16).unwrap_or(0x20);
        [v(0), v(2), v(4)]
    }

    /// A surface's picture: its buffer and the memory under it. It is kept
    /// until the next one replaces it: a buffer destroyed while its surface
    /// still shows it leaves the surface with nothing to show at its next
    /// commit, which is how the plate went missing.
    type Shown = (wl_shm_pool::WlShmPool, wl_buffer::WlBuffer);

    fn drop_shown(old: Option<Shown>) {
        if let Some((pool, buffer)) = old {
            buffer.destroy();
            pool.destroy();
        }
    }

    /// Put pixels on a surface through a fresh shared-memory buffer.
    fn show(surface: &wl_surface::WlSurface, shm: &wl_shm::WlShm, qh: &QueueHandle<St>, w: u32, h: u32, bytes: &[u8]) -> Result<Shown, String> {
        if w == 0 || h == 0 || bytes.len() != (w * h * 8) as usize {
            return Err(format!("no picture of {w} x {h} to show"));
        }
        let path = std::env::temp_dir().join(format!("ectodeck-hdr-{}-{}", std::process::id(), surface.id().protocol_id()));
        let mut file = std::fs::File::options().read(true).write(true).create(true).truncate(true).open(&path).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&path);
        file.write_all(bytes).map_err(|e| e.to_string())?;
        let pool = shm.create_pool(file.as_fd(), bytes.len() as i32, qh, ());
        let buffer = pool.create_buffer(0, w as i32, h as i32, (w * 8) as i32, wl_shm::Format::Abgr16161616, qh, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, w as i32, h as i32);
        surface.commit();
        Ok((pool, buffer))
    }

    fn run(display: usize, parent_ptr: usize, reference: f64, art: (String, String), rx: mpsc::Receiver<Msg>) -> Result<(), String> {
        let tree = resvg::usvg::Tree::from_str(&art.0, &resvg::usvg::Options::default()).map_err(|e| e.to_string())?;
        let device: serde_json::Value = serde_json::from_str(&art.1).map_err(|e| e.to_string())?;
        let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
        let conn = Connection::from_backend(backend);
        let mut queue: EventQueue<St> = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut st = St::default();
        queue.roundtrip(&mut st).map_err(|e| e.to_string())?;
        let (Some(compositor), Some(subc), Some(shm), Some(cm)) = (st.compositor.clone(), st.sub.clone(), st.shm.clone(), st.cm.clone()) else {
            return Err("the compositor lacks sub-surfaces, shared memory or color management".into());
        };
        let parent_id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), parent_ptr as *mut _) }.map_err(|e| format!("{e:?}"))?;
        let parent = wl_surface::WlSurface::from_id(&conn, parent_id).map_err(|e| format!("{e:?}"))?;

        let params = cm.create_parametric_creator(&qh, ());
        params.set_tf_named(wp_color_manager_v1::TransferFunction::St2084Pq);
        params.set_primaries_named(wp_color_manager_v1::Primaries::Bt2020);
        params.set_luminances(0, 10000, reference.round() as u32);
        let desc = params.create(&qh, ());
        while !st.ready && st.failed.is_none() {
            queue.blocking_dispatch(&mut st).map_err(|e| e.to_string())?;
        }
        if let Some(m) = st.failed.take() {
            return Err(format!("the compositor refused the image description: {m}"));
        }

        // one layer below the window's own surface, at the keyboard, carrying the keyboard
        let back = compositor.create_surface(&qh, ());
        let back_sub = subc.get_subsurface(&back, &parent, &qh, ());
        back_sub.set_desync();
        let empty = compositor.create_region(&qh, ());
        back.set_input_region(Some(&empty));
        let back_cm = cm.get_surface(&back, &qh, ());
        back_cm.set_image_description(&desc, wp_color_manager_v1::RenderIntent::Perceptual);
        back_sub.place_below(&parent);

        let enc = crate::hdr_draw::Encoder::new(reference);
        let mut layout: Option<Layout> = None;
        let mut lights: Vec<crate::hdr_draw::Light> = Vec::new();
        // the keyboard's pictures, made again only when its size or the ground's color changes
        let mut board: Option<((u32, u32, String), crate::hdr_draw::Board)> = None;
        let mut back_pic: Option<Shown> = None;
        let mut showing = false;
        eprintln!("ectodeck: HDR board: started, reference white {reference} nits");
        loop {
            let Ok(first) = rx.recv() else { break };
            // the most recent of each wins
            let mut changed = false;
            for m in std::iter::once(first).chain(rx.try_iter()) {
                match m {
                    Msg::Layout(l) => {
                        if let Some(f) = &l.lights {
                            lights = super::lights(f);
                        }
                        layout = Some(l);
                        changed = true;
                    }
                    Msg::Light(l) => {
                        if l != lights {
                            lights = l;
                            changed = true;
                        }
                    }
                }
            }
            let Some(l) = layout.as_ref() else { continue };
            if !changed || l.win_w == 0 || l.win_h == 0 {
                continue;
            }
            match l.board {
                Some([x, y, w, h]) if w >= 1.0 && h >= 1.0 => {
                    let key = (w.round() as u32, h.round() as u32, l.base.clone());
                    if board.as_ref().is_none_or(|(k, _)| *k != key) {
                        let t = std::time::Instant::now();
                        board = Some((key.clone(), crate::hdr_draw::Board::new(&tree, &device, hex(&l.base), key.0, key.1)?));
                        eprintln!("ectodeck: HDR keyboard drawn at {}x{} in {:?}", key.0, key.1, t.elapsed());
                    }
                    let (_, b) = board.as_ref().expect("made above");
                    let gain = if l.glow > reference { (l.glow / reference) as f32 } else { 1.0 };
                    let px = enc.encode(&b.compose(&lights, gain));
                    if std::env::var_os("ECTODECK_HDR_TEST").is_some() {
                        eprintln!("ectodeck: HDR keyboard {}x{} at ({x:.0}, {y:.0}), {} lit, glow {:.0} nits [{}]", b.w, b.h,
                            lights.iter().filter(|k| k.strength > 0.0).count(), l.glow, l.why);
                        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
                        let _ = std::fs::write(dir.join(format!("ectodeck-hdr-keyboard-{}x{}.raw", b.w, b.h)), &px);
                    }
                    back_sub.set_position(x.round() as i32, y.round() as i32);
                    let pic = show(&back, &shm, &qh, b.w, b.h, &px)?;
                    drop_shown(back_pic.replace(pic));
                    showing = true;
                }
                _ => {
                    if showing {
                        back.attach(None, 0, 0);
                        back.commit();
                        drop_shown(back_pic.take());
                        showing = false;
                    }
                }
            }
            conn.flush().map_err(|e| e.to_string())?;
            queue.dispatch_pending(&mut st).map_err(|e| e.to_string())?;
        }
        // the window's surface has gone (it was hidden), or the app is ending:
        // take this layer's own objects down with it
        drop_shown(back_pic.take());
        back_cm.destroy();
        back_sub.destroy();
        back.destroy();
        desc.destroy();
        let _ = conn.flush();
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use imp::layout;

#[cfg(target_os = "linux")]
pub use imp::light;

#[cfg(not(target_os = "linux"))]
pub fn light(_: Vec<crate::hdr_draw::Light>) {}

#[cfg(not(target_os = "linux"))]
pub fn layout(_: &tauri::WebviewWindow, _: Layout, _: f64, _: (String, String)) -> Result<(), String> {
    Err("HDR lighting is only built for Linux so far".into())
}
