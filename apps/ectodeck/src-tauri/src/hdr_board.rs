//! The keyboard drawing in true HDR, drawn by Ectodeck itself under a
//! see-through webview.
//!
//! WebKitGTK only draws SDR, so anything brighter than the desktop's white has
//! to come from a surface of Ectodeck's own. Two Wayland sub-surfaces sit
//! BELOW the window's own surface, on GTK's own connection (the only way to
//! relate them to it): the backdrop, the window's ground with its rounded
//! corners, and the board: the layers of the keyboard drawing that lie under
//! the light, at its place on the page. The page keeps drawing everything
//! else as it does now (the deck with its wells cut out, the caps, the
//! legends and everything interactive) and turns see-through where these
//! layers show (html.hdr), so the light shows through the wells as it always
//! has, only brighter.
//!
//! Both are declared PQ with BT.2020 primaries at the screen's reference white,
//! and SDR pictures are converted the way KWin shows the webview itself
//! (gamma 2.2 at the reference white), so the two meet without a seam.
//!
//! Stage 1: the ground and the plate (board.svg). The halos, and light under
//! legends cut out of the caps, follow.

use serde::Deserialize;

/// Where things are, in the window's pixels, as the page reports them.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Layout {
    pub win_w: u32,
    pub win_h: u32,
    /// the window's corner radius; 0 when maximised
    pub radius: f64,
    /// the ground's colour, as the page's CSS has it (#rrggbb)
    pub base: String,
    /// the board's rect, or none when the keyboard page isn't showing
    pub board: Option<[f64; 4]>,
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
    static CHANNEL: Mutex<Option<(mpsc::Sender<Layout>, u64)>> = Mutex::new(None);
    /// How many times the window has been shown. Each showing has a new
    /// surface, which Wayland may give the old one's id and GTK its address,
    /// so the count is what tells them apart.
    static SHOWINGS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static COUNTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// Show the layout, starting the HDR layer the first time. Only the main
    /// window has the layer, and a window with no size (hidden, or not laid out
    /// yet) has nothing to show: a buffer of no size is a protocol error, which
    /// ends the whole app.
    pub fn layout(window: &tauri::WebviewWindow, layout: Layout, reference: f64, art: [String; 1]) -> Result<(), String> {
        if window.label() != "main" || layout.win_w == 0 || layout.win_h == 0 {
            return Ok(());
        }
        // The window's surface as it is now. GTK makes a new one each time the
        // window is shown again (closing it only hides it, to the tray), and a
        // hidden window has none, so there is nothing to draw on until it shows.
        let Ok((display, surface, showing)) = gtk_pointers(window) else { return Ok(()) };
        let mut ch = CHANNEL.lock().map_err(|e| e.to_string())?;
        if let Some((tx, on)) = ch.as_ref() {
            if *on == showing && tx.send(layout.clone()).is_ok() {
                return Ok(());
            }
        }
        // a new surface, or none yet: the old layer, if any, ends with its own
        // (dropping its sender ends its thread), and one starts on this one
        *ch = None;
        let (tx, rx) = mpsc::channel::<Layout>();
        tx.send(layout).map_err(|e| e.to_string())?;
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
                    let gdk_window = gtk_window.window().ok_or("the window is not realised")?;
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

    /// PQ signal for nits
    fn pq(nits: f64) -> f64 {
        let (m1, m2) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0);
        let (c1, c2, c3) = (3424.0 / 4096.0, 2413.0 / 4096.0 * 32.0, 2392.0 / 4096.0 * 32.0);
        let y = (nits.max(0.0) / 10000.0).powf(m1);
        ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
    }

    /// An 8-bit sRGB value to 16-bit PQ: gamma 2.2 at the reference white,
    /// BT.709 into BT.2020, as KWin shows the webview's own pixels.
    struct Convert {
        lut: Vec<[f64; 3]>, // per 8-bit channel value, its linear contribution to each BT.2020 channel is not separable; keep linear 709 per value
        reference: f64,
    }
    impl Convert {
        fn new(reference: f64) -> Self {
            Convert { lut: (0..256).map(|v| { let l = (v as f64 / 255.0).powf(2.2); [l, l, l] }).collect(), reference }
        }
        /// premultiplied RGBA8 (as resvg makes it) to premultiplied ABGR16161616
        fn pixel(&self, r: u8, g: u8, b: u8, a: u8, out: &mut Vec<u8>) {
            if a == 0 {
                out.extend_from_slice(&[0; 8]);
                return;
            }
            let af = a as f64 / 255.0;
            // unpremultiply, decode, convert primaries, encode, premultiply again
            let un = |c: u8| ((c as f64 / af).round().clamp(0.0, 255.0)) as usize;
            let l = [self.lut[un(r)][0], self.lut[un(g)][1], self.lut[un(b)][2]];
            let m = [[0.6274, 0.3293, 0.0433], [0.0691, 0.9195, 0.0114], [0.0164, 0.0880, 0.8956]];
            for i in 0..3 {
                let lin = m[i][0] * l[0] + m[i][1] * l[1] + m[i][2] * l[2];
                let v = (pq(lin * self.reference) * af * 65535.0).round().clamp(0.0, 65535.0) as u16;
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&((af * 65535.0).round() as u16).to_le_bytes());
        }
    }

    /// The ground: the base colour over the whole window, corners rounded.
    fn backdrop(w: u32, h: u32, radius: f64, base: [u8; 3], conv: &Convert) -> Vec<u8> {
        let mut out = Vec::with_capacity((w * h * 8) as usize);
        for y in 0..h {
            for x in 0..w {
                // coverage of a rounded corner, antialiased over one pixel
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                let cx = if fx < radius { radius - fx } else if fx > w as f64 - radius { fx - (w as f64 - radius) } else { 0.0 };
                let cy = if fy < radius { radius - fy } else if fy > h as f64 - radius { fy - (h as f64 - radius) } else { 0.0 };
                let a = if cx > 0.0 && cy > 0.0 { (radius - (cx * cx + cy * cy).sqrt() + 0.5).clamp(0.0, 1.0) } else { 1.0 };
                let a8 = (a * 255.0).round() as u8;
                let p = |c: u8| ((c as f64) * a).round() as u8;
                conv.pixel(p(base[0]), p(base[1]), p(base[2]), a8, &mut out);
            }
        }
        out
    }

    /// The plate: board.svg, fitted into the board's rect keeping its
    /// proportions and centred, as the page's <img> shows it.
    fn board(w: u32, h: u32, art: &[String; 1], conv: &Convert) -> Result<Vec<u8>, String> {
        let mut pix = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or("board too small")?;
        for svg in art {
            let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).map_err(|e| e.to_string())?;
            let s = tree.size();
            let k = (w as f32 / s.width()).min(h as f32 / s.height());
            let (tx, ty) = ((w as f32 - s.width() * k) / 2.0, (h as f32 - s.height() * k) / 2.0);
            resvg::render(&tree, resvg::tiny_skia::Transform::from_row(k, 0.0, 0.0, k, tx, ty), &mut pix.as_mut());
        }
        let mut out = Vec::with_capacity((w * h * 8) as usize);
        for p in pix.data().chunks_exact(4) {
            conv.pixel(p[0], p[1], p[2], p[3], &mut out);
        }
        Ok(out)
    }

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

    fn run(display: usize, parent_ptr: usize, reference: f64, art: [String; 1], rx: mpsc::Receiver<Layout>) -> Result<(), String> {
        let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
        let conn = Connection::from_backend(backend);
        let mut queue: EventQueue<St> = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut st = St::default();
        queue.roundtrip(&mut st).map_err(|e| e.to_string())?;
        let (Some(compositor), Some(subc), Some(shm), Some(cm)) = (st.compositor.clone(), st.sub.clone(), st.shm.clone(), st.cm.clone()) else {
            return Err("the compositor lacks sub-surfaces, shared memory or colour management".into());
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

        // both below the window's own surface: the board over the backdrop
        let make = |qh: &QueueHandle<St>| {
            let s = compositor.create_surface(qh, ());
            let sub = subc.get_subsurface(&s, &parent, qh, ());
            sub.set_desync();
            let empty = compositor.create_region(qh, ());
            s.set_input_region(Some(&empty));
            let c = cm.get_surface(&s, qh, ());
            c.set_image_description(&desc, wp_color_manager_v1::RenderIntent::Perceptual);
            (s, sub, c)
        };
        let (back, back_sub, back_cm) = make(&qh);
        let (brd, brd_sub, brd_cm) = make(&qh);
        back_sub.set_position(0, 0);
        back_sub.place_below(&parent);
        brd_sub.place_above(&back);
        brd_sub.place_below(&parent);

        let conv = Convert::new(reference);
        let mut shown: Option<Layout> = None;
        let mut board_size: Option<(u32, u32)> = None;
        let (mut back_pic, mut board_pic): (Option<Shown>, Option<Shown>) = (None, None);
        eprintln!("ectodeck: HDR board: started, reference white {reference} nits");
        loop {
            let Ok(mut l) = rx.recv() else { break };
            // the most recent layout wins
            while let Ok(next) = rx.try_recv() {
                l = next;
            }
            if l.win_w == 0 || l.win_h == 0 {
                continue;
            }
            let prev = shown.clone();
            if prev.as_ref().is_none_or(|p| p.win_w != l.win_w || p.win_h != l.win_h || p.radius != l.radius || p.base != l.base) {
                let pic = show(&back, &shm, &qh, l.win_w, l.win_h, &backdrop(l.win_w, l.win_h, l.radius, hex(&l.base), &conv))?;
                drop_shown(back_pic.replace(pic));
            }
            match l.board {
                Some([x, y, w, h]) if w >= 1.0 && h >= 1.0 => {
                    let size = (w.round() as u32, h.round() as u32);
                    brd_sub.set_position(x.round() as i32, y.round() as i32);
                    if board_size != Some(size) {
                        let pic = show(&brd, &shm, &qh, size.0, size.1, &board(size.0, size.1, &art, &conv)?)?;
                        drop_shown(board_pic.replace(pic));
                        board_size = Some(size);
                    } else {
                        brd.commit();
                    }
                }
                _ => {
                    if board_size.is_some() {
                        brd.attach(None, 0, 0);
                        brd.commit();
                        drop_shown(board_pic.take());
                        board_size = None;
                    }
                }
            }
            // the position and stacking apply with the window's next commit
            conn.flush().map_err(|e| e.to_string())?;
            queue.dispatch_pending(&mut st).map_err(|e| e.to_string())?;
            shown = Some(l);
        }
        // the window's surface has gone (it was hidden), or the app is ending:
        // take this layer's own objects down with it
        drop_shown(back_pic.take());
        drop_shown(board_pic.take());
        for cm in [&brd_cm, &back_cm] {
            cm.destroy();
        }
        for sub in [&brd_sub, &back_sub] {
            sub.destroy();
        }
        for s in [&brd, &back] {
            s.destroy();
        }
        desc.destroy();
        let _ = conn.flush();
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use imp::layout;

#[cfg(not(target_os = "linux"))]
pub fn layout(_: &tauri::WebviewWindow, _: Layout, _: f64, _: [String; 2]) -> Result<(), String> {
    Err("HDR lighting is only built for Linux so far".into())
}
