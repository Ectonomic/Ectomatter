//! How bright each screen can go above its white, for Settings › HDR.
//!
//! On Linux this asks the compositor through the Wayland colour-management
//! protocol for each screen's own image description, whose luminances give
//! the screen's reference white (the desktop's white) and the peak it can
//! reach. The room between them is what HDR lighting uses: a screen in HDR
//! mode has it, and so does an SDR screen whose brightness is turned down,
//! since KWin keeps the backlight's headroom (its EDR). KWin describes every
//! screen as gamma 2.2, even in HDR mode, so the transfer function is not
//! read. Elsewhere there is no such question to ask yet, so the list is empty
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
    #[derive(Default)]
    struct App {
        cm: Option<wp_color_manager_v1::WpColorManagerV1>,
        outputs: Vec<wl_output::WlOutput>,
        screens: Vec<Screen>,
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
    delegate_noop!(App: ignore wp_color_manager_v1::WpColorManagerV1);
    delegate_noop!(App: ignore wp_color_management_output_v1::WpColorManagementOutputV1);
    delegate_noop!(App: ignore wp_image_description_v1::WpImageDescriptionV1);

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
        let desc = cmo.get_image_description(&qh, ());
        keep.push((desc.get_information(&qh, i), desc, cmo));
    }
    for _ in 0..20 {
        if app.screens.iter().all(|s| s.done) || queue.roundtrip(&mut app).is_err() {
            break;
        }
    }
    app.screens.iter().map(|s| json!({ "name": s.name, "description": s.description, "reference": s.reference, "peak": s.peak })).collect()
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
