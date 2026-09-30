//! SPIKE: a see-through HDR layer over the keyboard drawing.
//!
//! WebKitGTK only draws SDR, so light brighter than the desktop's white has
//! to come from a surface of Ectodeck's own. This attaches a Wayland
//! sub-surface to the window's own surface (on GTK's own Wayland connection,
//! which is the only way to relate the two), declares it PQ with BT.2020
//! primaries at the screen's reference white, lets pointer input fall through
//! to the webview, and draws a halo round one key in premultiplied 16-bit
//! ABGR, so its transparency is smooth.
//!
//! This is a feasibility test: one fixed red halo at the given rect.

#[cfg(target_os = "linux")]
pub fn spike(window: &tauri::WebviewWindow, x: f64, y: f64, w: f64, h: f64, peak: f64, reference: f64) -> Result<(), String> {
    use gtk::glib::translate::ToGlibPtr;
    use gtk::prelude::*;
    use std::sync::mpsc;

    unsafe extern "C" {
        fn gdk_wayland_window_get_wl_surface(window: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn gdk_wayland_display_get_wl_display(display: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    // GTK objects are only touched on the main thread; the pointers are then
    // handed to a thread of our own that owns the sub-surface
    let (tx, rx) = mpsc::channel::<Result<(usize, usize), String>>();
    let win = window.clone();
    window
        .run_on_main_thread(move || {
            let r = (|| {
                let gtk_window = win.gtk_window().map_err(|e| e.to_string())?;
                let gdk_window = gtk_window.window().ok_or("the window is not realised")?;
                let display = gdk_window.display();
                let d: *mut gtk::gdk::ffi::GdkDisplay = display.to_glib_none().0;
                let s: *mut gtk::gdk::ffi::GdkWindow = gdk_window.to_glib_none().0;
                let wl_display = unsafe { gdk_wayland_display_get_wl_display(d as *mut _) };
                let wl_surface = unsafe { gdk_wayland_window_get_wl_surface(s as *mut _) };
                if wl_display.is_null() || wl_surface.is_null() {
                    return Err("not running on Wayland".to_string());
                }
                Ok((wl_display as usize, wl_surface as usize))
            })();
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
    let (display, surface) = rx.recv().map_err(|e| e.to_string())??;
    std::thread::Builder::new()
        .name("hdr-overlay".into())
        .spawn(move || {
            if let Err(e) = run(display, surface, x, y, w, h, peak, reference) {
                eprintln!("ectodeck: HDR overlay: {e}");
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn spike(_: &tauri::WebviewWindow, _: f64, _: f64, _: f64, _: f64, _: f64, _: f64) -> Result<(), String> {
    Err("HDR lighting is only built for Linux so far".into())
}

#[cfg(target_os = "linux")]
#[allow(clippy::too_many_arguments)]
fn run(display: usize, parent_ptr: usize, x: f64, y: f64, w: f64, h: f64, peak: f64, reference: f64) -> Result<(), String> {
    use std::io::Write;
    use std::os::fd::AsFd;
    use wayland_backend::client::{Backend, ObjectId};
    use wayland_client::protocol::{wl_buffer, wl_compositor, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface};
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
    use wayland_protocols::wp::color_management::v1::client::{
        wp_color_management_surface_v1, wp_color_manager_v1, wp_image_description_creator_params_v1, wp_image_description_v1,
    };

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

    // GTK's own connection, joined as a guest, with a queue of our own
    let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
    let conn = Connection::from_backend(backend);
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut st = St::default();
    queue.roundtrip(&mut st).map_err(|e| e.to_string())?;
    let (Some(compositor), Some(subc), Some(shm), Some(cm)) = (st.compositor.clone(), st.sub.clone(), st.shm.clone(), st.cm.clone()) else {
        return Err("the compositor lacks sub-surfaces, shared memory or colour management".into());
    };
    let parent_id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), parent_ptr as *mut _) }.map_err(|e| format!("{e:?}"))?;
    let parent = wl_surface::WlSurface::from_id(&conn, parent_id).map_err(|e| format!("{e:?}"))?;

    // the halo reaches this far past the cap, as the drawing's halo does
    let margin = (h * 0.6).round();
    let (ow, oh) = ((w + 2.0 * margin).round() as i32, (h + 2.0 * margin).round() as i32);
    let child = compositor.create_surface(&qh, ());
    let subsurface = subc.get_subsurface(&child, &parent, &qh, ());
    subsurface.set_position((x - margin).round() as i32, (y - margin).round() as i32);
    subsurface.place_above(&parent);
    subsurface.set_desync();
    let empty = compositor.create_region(&qh, ());
    child.set_input_region(Some(&empty));

    let params = cm.create_parametric_creator(&qh, ());
    params.set_tf_named(wp_color_manager_v1::TransferFunction::St2084Pq);
    params.set_primaries_named(wp_color_manager_v1::Primaries::Bt2020);
    params.set_luminances(0, 10000, reference.round() as u32);
    let desc = params.create(&qh, ());
    while !st.ready && st.failed.is_none() {
        queue.blocking_dispatch(&mut st).map_err(|e| e.to_string())?;
    }
    if let Some(m) = st.failed {
        return Err(format!("the compositor refused the image description: {m}"));
    }
    let cms = cm.get_surface(&child, &qh, ());
    cms.set_image_description(&desc, wp_color_manager_v1::RenderIntent::Perceptual);

    // the halo: the LED's light spilling from under the cap, falling off with
    // distance from its edge; nothing drawn over the cap itself
    let pq = |nits: f64| {
        let (m1, m2) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0);
        let (c1, c2, c3) = (3424.0 / 4096.0, 2413.0 / 4096.0 * 32.0, 2392.0 / 4096.0 * 32.0);
        let y = (nits.max(0.0) / 10000.0).powf(m1);
        ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
    };
    let led709 = [1.0f64, 0.0028, 0.0]; // red, linear
    let m = [[0.6274, 0.3293, 0.0433], [0.0691, 0.9195, 0.0114], [0.0164, 0.0880, 0.8956]];
    let led: [f64; 3] = [0, 1, 2].map(|i| m[i][0] * led709[0] + m[i][1] * led709[1] + m[i][2] * led709[2]);
    let top = led709.iter().cloned().fold(0.0, f64::max);
    let reach = h * 0.6;
    let mut bytes = Vec::with_capacity((ow * oh * 8) as usize);
    for py in 0..oh {
        for px in 0..ow {
            let dx = ((px as f64 + 0.5 - ow as f64 / 2.0).abs() - w / 2.0).max(0.0);
            let dy = ((py as f64 + 0.5 - oh as f64 / 2.0).abs() - h / 2.0).max(0.0);
            let d = (dx * dx + dy * dy).sqrt();
            let a = if d > 0.0 { (-d / (reach * 0.28)).exp() * 0.9 } else { 0.0 };
            // premultiplied: each channel's PQ signal at the halo's full
            // brightness, times the coverage
            let ch = |c: f64| ((pq(c / top * peak) * a) * 65535.0).round().clamp(0.0, 65535.0) as u16;
            for v in [ch(led[0]), ch(led[1]), ch(led[2]), (a * 65535.0).round() as u16] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    let path = std::env::temp_dir().join(format!("ectodeck-hdr-{}", std::process::id()));
    let mut file = std::fs::File::options().read(true).write(true).create(true).truncate(true).open(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&path);
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    let pool = shm.create_pool(file.as_fd(), bytes.len() as i32, &qh, ());
    let buffer = pool.create_buffer(0, ow, oh, ow * 8, wl_shm::Format::Abgr16161616, &qh, ());
    child.attach(Some(&buffer), 0, 0);
    child.damage_buffer(0, 0, ow, oh);
    child.commit();
    // the parent applies the sub-surface's position and stacking on its next
    // commit, which the webview makes whenever it repaints
    eprintln!("ectodeck: HDR overlay: {ow}x{oh} at ({}, {}), peak {peak} nits over a {reference}-nit white", (x - margin).round(), (y - margin).round());
    loop {
        queue.blocking_dispatch(&mut st).map_err(|e| e.to_string())?;
    }
}
