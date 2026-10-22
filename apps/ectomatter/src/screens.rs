//! How bright each screen can go above its white, for Settings › HDR.
//!
//! On Linux this asks the compositor through the Wayland color-management
//! protocol for each screen's own image description, whose luminances give
//! the screen's reference white (the desktop's white) and the peak it can
//! reach. The room between them is what HDR lighting uses: a screen in HDR
//! mode has it, and so does an SDR screen whose brightness is turned down,
//! since KWin keeps the backlight's headroom (its EDR). KWin describes every
//! screen as gamma 2.2, even in HDR mode, so the transfer function is not
//! read. The white a screen's user set is KDE's word (kde_output): KWin's
//! reference is lower while the desktop is idle and the screen dimmed.
//! Elsewhere there is no such question to ask yet, so the list is empty
//! and Settings says it cannot tell.

use serde_json::{Value, json};

#[cfg(target_os = "linux")]
pub fn screens() -> Vec<Value> {
    use wayland_client::protocol::{wl_output, wl_registry};
    use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
    use wayland_protocols::wp::color_management::v1::client::{
        wp_color_management_output_v1, wp_color_manager_v1, wp_image_description_info_v1, wp_image_description_v1,
    };

    #[derive(Default)]
    struct Screen {
        name: String,
        description: String,
        reference: f64,
        peak: f64,
        done: bool,
    }
    use crate::kde_output::client::{kde_output_device_mode_v2::KdeOutputDeviceModeV2, kde_output_device_registry_v2::{self, KdeOutputDeviceRegistryV2}, kde_output_device_v2::{self, KdeOutputDeviceV2}};

    #[derive(Default)]
    struct App {
        cm: Option<wp_color_manager_v1::WpColorManagerV1>,
        outputs: Vec<wl_output::WlOutput>,
        screens: Vec<Screen>,
        /// what KDE says of each screen, and whether its word is whole
        kde: Vec<(KdeOutputDeviceV2, crate::kde_output::Device, bool)>,
    }
    impl Dispatch<wl_registry::WlRegistry, ()> for App {
        fn event(app: &mut Self, reg: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
            if let wl_registry::Event::Global { name, interface, version } = event {
                match interface.as_str() {
                    "wl_output" => {
                        let i = app.outputs.len();
                        app.outputs.push(reg.bind(name, version.min(4), qh, i));
                        app.screens.push(Screen::default());
                    }
                    "wp_color_manager_v1" => app.cm = Some(reg.bind(name, 1, qh, ())),
                    "kde_output_device_registry_v2" if version >= 21 => {
                        let _: KdeOutputDeviceRegistryV2 = reg.bind(name, version.min(23), qh, ());
                    }
                    _ => {}
                }
            }
        }
    }
    impl Dispatch<wl_output::WlOutput, usize> for App {
        fn event(app: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, i: &usize, _: &Connection, _: &QueueHandle<Self>) {
            match event {
                wl_output::Event::Name { name } => app.screens[*i].name = name,
                wl_output::Event::Description { description } => app.screens[*i].description = description,
                _ => {}
            }
        }
    }
    impl Dispatch<wp_image_description_info_v1::WpImageDescriptionInfoV1, usize> for App {
        fn event(app: &mut Self, _: &wp_image_description_info_v1::WpImageDescriptionInfoV1, event: wp_image_description_info_v1::Event, i: &usize, _: &Connection, _: &QueueHandle<Self>) {
            let s = &mut app.screens[*i];
            match event {
                wp_image_description_info_v1::Event::Luminances { max_lum, reference_lum, .. } => {
                    s.peak = max_lum as f64;
                    s.reference = reference_lum as f64;
                }
                wp_image_description_info_v1::Event::Done => s.done = true,
                _ => {}
            }
        }
    }
    impl Dispatch<KdeOutputDeviceRegistryV2, ()> for App {
        fn event(app: &mut Self, _: &KdeOutputDeviceRegistryV2, e: kde_output_device_registry_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let kde_output_device_registry_v2::Event::Output { output } = e {
                app.kde.push((output, crate::kde_output::Device::default(), false));
            }
        }
        wayland_client::event_created_child!(App, KdeOutputDeviceRegistryV2, [
            kde_output_device_registry_v2::EVT_OUTPUT_OPCODE => (KdeOutputDeviceV2, ()),
        ]);
    }
    impl Dispatch<KdeOutputDeviceV2, ()> for App {
        fn event(app: &mut Self, dev: &KdeOutputDeviceV2, e: kde_output_device_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let Some(k) = app.kde.iter_mut().find(|k| &k.0 == dev) {
                k.2 |= k.1.take(e);
            }
        }
        wayland_client::event_created_child!(App, KdeOutputDeviceV2, [
            kde_output_device_v2::EVT_MODE_OPCODE => (KdeOutputDeviceModeV2, ()),
        ]);
    }
    delegate_noop!(App: ignore KdeOutputDeviceModeV2);
    delegate_noop!(App: ignore wp_color_manager_v1::WpColorManagerV1);
    delegate_noop!(App: ignore wp_color_management_output_v1::WpColorManagementOutputV1);
    // a screen's description asked for its levels once KWin has it ready (a
    // screen turned off has none: asking a failed one is a protocol error)
    impl Dispatch<wp_image_description_v1::WpImageDescriptionV1, usize> for App {
        fn event(app: &mut Self, desc: &wp_image_description_v1::WpImageDescriptionV1, e: wp_image_description_v1::Event, i: &usize, _: &Connection, qh: &QueueHandle<Self>) {
            match e {
                wp_image_description_v1::Event::Ready { .. } | wp_image_description_v1::Event::Ready2 { .. } => {
                    desc.get_information(qh, *i);
                }
                wp_image_description_v1::Event::Failed { .. } => app.screens[*i].done = true,
                _ => {}
            }
        }
    }

    let Ok(conn) = Connection::connect_to_env() else { return vec![] };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut app = App::default();
    if queue.roundtrip(&mut app).is_err() || queue.roundtrip(&mut app).is_err() {
        return vec![];
    }
    let Some(cm) = app.cm.clone() else { return vec![] };
    let mut keep = Vec::new();
    for (i, o) in app.outputs.iter().enumerate() {
        let cmo = cm.get_output(o, &qh, ());
        keep.push((cmo.get_image_description(&qh, i), cmo));
    }
    for _ in 0..20 {
        if (app.screens.iter().all(|s| s.done) && app.kde.iter().all(|k| k.2)) || queue.roundtrip(&mut app).is_err() {
            break;
        }
    }
    // each screen's white: the one its user set, where KDE says it
    app.screens
        .iter()
        .map(|s| {
            let kde = app.kde.iter().map(|k| &k.1).find(|d| d.name == s.name);
            json!({ "name": s.name, "description": s.description, "reference": s.reference, "white": crate::kde_output::white(s.reference, kde, None), "peak": s.peak })
        })
        .collect()
}

#[cfg(not(target_os = "linux"))]
pub fn screens() -> Vec<Value> {
    vec![]
}

#[cfg(test)]
mod tests {
    /// Needs a running Wayland session: `cargo test screens_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn screens_live() {
        println!("{:?}", super::screens());
    }
}
